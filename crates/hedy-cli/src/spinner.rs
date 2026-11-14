//! A small spinner on stderr while something slow runs (checking logins starts a
//! process per account). Only in a terminal with colors, and only if the work
//! takes long enough to notice.

use std::io::{IsTerminal, Write};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use crate::style::Style;

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
/// Quick work finishes before the spinner shows, so nothing flickers.
const DELAY: Duration = Duration::from_millis(150);
const TICK: Duration = Duration::from_millis(80);

pub fn while_running<T>(label: &str, work: impl FnOnce() -> T) -> T {
    let style = Style::for_stderr();
    if !style.enabled() || !std::io::stderr().is_terminal() {
        return work();
    }
    let (done, finished) = mpsc::channel::<()>();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            if finished.recv_timeout(DELAY) != Err(RecvTimeoutError::Timeout) {
                return;
            }
            let mut stderr = std::io::stderr();
            for frame in FRAMES.iter().cycle() {
                let _ = write!(stderr, "\r  {} {}", style.accent(frame), style.muted(label));
                let _ = stderr.flush();
                if finished.recv_timeout(TICK) != Err(RecvTimeoutError::Timeout) {
                    break;
                }
            }
            // Clears the line.
            let _ = write!(stderr, "\r\x1b[2K");
            let _ = stderr.flush();
        });
        let result = work();
        let _ = done.send(());
        result
    })
}
