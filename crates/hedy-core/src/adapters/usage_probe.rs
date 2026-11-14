//! Plan-usage sources that need more than one captured command: Codex's
//! app-server (JSON-RPC over stdio) and Moonshot's balance endpoint.
//! Both only read; neither sends anything to a model.

use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::process::{command, launch_error};
use crate::application::ports::{PortError, PortResult, Secret, UsageProbe};
use crate::domain::LaunchSpec;
use crate::tr;

pub struct SystemUsageProbe;

impl UsageProbe for SystemUsageProbe {
    fn codex_rate_limits(&self, spec: &LaunchSpec, timeout: Duration) -> PortResult<String> {
        let mut child = command(spec)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| launch_error(spec, e))?;
        let mut stdin = child.stdin.take().expect("stdin is piped");
        let stdout = child.stdout.take().expect("stdout is piped");
        let (lines, received) = mpsc::channel::<Value>();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(msg) = serde_json::from_str::<Value>(&line)
                    && lines.send(msg).is_err()
                {
                    break;
                }
            }
        });
        let deadline = Instant::now() + timeout;
        let mut send = |msg: Value| -> PortResult<()> {
            let mut line = msg.to_string();
            line.push('\n');
            stdin
                .write_all(line.as_bytes())
                .and_then(|()| stdin.flush())
                .map_err(|e| PortError::new(format!("Codex: {e}")))
        };
        // The docs: one `initialize` per connection, then `initialized`;
        // any other request before that gets "Not initialized".
        let result = (|| {
            send(
                json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                    "clientInfo": {"name": "hedy", "title": "Hedy", "version": env!("CARGO_PKG_VERSION")}
                }}),
            )?;
            wait_for(&received, 1, deadline)?;
            send(json!({"jsonrpc": "2.0", "method": "initialized"}))?;
            send(json!({"jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read"}))?;
            wait_for(&received, 2, deadline)
        })();
        let _ = child.kill();
        let _ = child.wait();
        result.map(|v| v.to_string())
    }

    fn moonshot_balance(
        &self,
        base_url: &str,
        api_key: &Secret,
        timeout: Duration,
    ) -> PortResult<String> {
        let http: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build()
            .into();
        let url = format!("{}/users/me/balance", base_url.trim_end_matches('/'));
        let mut response = http
            .get(&url)
            .header("Authorization", &format!("Bearer {}", api_key.expose()))
            .call()
            .map_err(|e| PortError::new(format!("{e}")))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(PortError::new(format!("HTTP {status}")));
        }
        response
            .body_mut()
            .read_to_string()
            .map_err(|e| PortError::new(format!("{e}")))
    }
}

/// The `result` of the response with `id`; notifications are skipped.
fn wait_for(received: &mpsc::Receiver<Value>, id: u64, deadline: Instant) -> PortResult<Value> {
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let msg = received.recv_timeout(left).map_err(|_| {
            PortError::new(tr!(
                "Codex no respondió a tiempo",
                "Codex did not answer in time"
            ))
        })?;
        if msg.get("method").is_some() || msg["id"].as_u64() != Some(id) {
            continue;
        }
        if let Some(error) = msg.get("error") {
            let message = error["message"].as_str().unwrap_or("error");
            return Err(PortError::new(format!("Codex: {message}")));
        }
        return Ok(msg["result"].clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plays `codex app-server`: answers `account/rateLimits/read` only after
    /// the handshake, like the real one ("Not initialized" otherwise).
    const FAKE_APP_SERVER: &str = r#"
import json, sys
def out(obj): print(json.dumps(obj), flush=True)
ready = False
for line in sys.stdin:
    msg = json.loads(line)
    method, mid = msg.get("method"), msg.get("id")
    if method == "initialize":
        out({"jsonrpc": "2.0", "id": mid, "result": {"userAgent": "fake"}})
    elif method == "initialized":
        ready = True
        out({"jsonrpc": "2.0", "method": "account/updated", "params": {}})
    elif method == "account/rateLimits/read":
        if not ready:
            out({"jsonrpc": "2.0", "id": mid, "error": {"code": -32600, "message": "Not initialized"}})
        elif sys.argv[1] == "logged-out":
            out({"jsonrpc": "2.0", "id": mid, "error": {"code": -32600, "message": "chatgpt authentication required to read rate limits"}})
        else:
            out({"jsonrpc": "2.0", "id": mid, "result": {"rateLimits": {"limitId": "codex",
                "primary": {"usedPercent": 25, "windowDurationMins": 300, "resetsAt": 1730947200},
                "secondary": None}}})
"#;

    fn spec(dir: &std::path::Path, mode: &str) -> LaunchSpec {
        let script = dir.join("fake_app_server.py");
        std::fs::write(&script, FAKE_APP_SERVER).unwrap();
        LaunchSpec::program(
            "python3",
            vec![
                "-I".into(),
                script.to_string_lossy().into_owned(),
                mode.into(),
            ],
        )
    }

    #[test]
    fn reads_rate_limits_after_the_handshake() {
        let tmp = tempfile::tempdir().unwrap();
        let json = SystemUsageProbe
            .codex_rate_limits(&spec(tmp.path(), "ok"), Duration::from_secs(10))
            .unwrap();
        let usage = crate::domain::plan_usage::parse_codex_rate_limits(&json, 1).unwrap();
        assert_eq!(usage.session().unwrap().percent, 25.0);
    }

    #[test]
    fn errors_and_silence_fail() {
        let tmp = tempfile::tempdir().unwrap();
        let err = SystemUsageProbe
            .codex_rate_limits(&spec(tmp.path(), "logged-out"), Duration::from_secs(10))
            .unwrap_err();
        assert!(err.message.contains("authentication required"), "{err}");

        let silent = LaunchSpec::program("sh", vec!["-c".into(), "sleep 5".into()]);
        let started = Instant::now();
        assert!(
            SystemUsageProbe
                .codex_rate_limits(&silent, Duration::from_millis(300))
                .is_err()
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn moonshot_balance_sends_the_key() {
        use std::io::Read;
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 2048];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let body = r#"{"code":0,"data":{"available_balance":49.5,"voucher_balance":46.5,"cash_balance":3.0},"scode":"0x0","status":true}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            request
        });
        let body = SystemUsageProbe
            .moonshot_balance(&url, &Secret::new("sk-test"), Duration::from_secs(10))
            .unwrap();
        assert!(body.contains("available_balance"));
        let request = server.join().unwrap();
        assert!(request.starts_with("GET /v1/users/me/balance"), "{request}");
        assert!(
            request
                .to_lowercase()
                .contains("authorization: bearer sk-test")
        );
    }
}
