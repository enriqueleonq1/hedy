//! Changes of a task, parsed from `git diff` output for the "Cambios" panel.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    /// Previous path for renames.
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub subject: String,
}

fn strip_prefix(path: &str) -> String {
    let path = path.trim_matches('"');
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path)
        .to_owned()
}

/// `@@ -12,7 +12,9 @@ fn x` → (12, 12).
fn hunk_starts(header: &str) -> (u32, u32) {
    let mut parts = header.split_whitespace().skip(1);
    let start = |part: Option<&str>| {
        part.and_then(|p| p[1..].split(',').next()?.parse().ok())
            .unwrap_or(0)
    };
    (start(parts.next()), start(parts.next()))
}

/// Parses `git diff` (unified, no color) into files, hunks and numbered lines.
pub fn parse_unified(diff: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let (mut old_no, mut new_no) = (0u32, 0u32);

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = rest.rsplit_once(" b/").map_or(rest, |(_, b)| b);
            files.push(FileDiff {
                path: path.to_owned(),
                old_path: None,
                status: FileStatus::Modified,
                additions: 0,
                deletions: 0,
                binary: false,
                hunks: Vec::new(),
            });
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };

        if file.hunks.is_empty() {
            if line.starts_with("new file mode") {
                file.status = FileStatus::Added;
            } else if line.starts_with("deleted file mode") {
                file.status = FileStatus::Deleted;
            } else if let Some(from) = line.strip_prefix("rename from ") {
                file.status = FileStatus::Renamed;
                file.old_path = Some(from.to_owned());
            } else if let Some(to) = line.strip_prefix("rename to ") {
                file.path = to.to_owned();
            } else if line.starts_with("Binary files ") {
                file.binary = true;
            } else if let Some(to) = line.strip_prefix("+++ ") {
                if to != "/dev/null" {
                    file.path = strip_prefix(to);
                }
            } else if line.starts_with("--- ") {
                // The new path (+++) is the one shown.
            }
        }

        if line.starts_with("@@") {
            (old_no, new_no) = hunk_starts(line);
            file.hunks.push(Hunk {
                header: line.to_owned(),
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = file.hunks.last_mut() else {
            continue;
        };
        let (kind, text) = match line.chars().next() {
            Some('+') => (LineKind::Added, &line[1..]),
            Some('-') => (LineKind::Removed, &line[1..]),
            Some(' ') => (LineKind::Context, &line[1..]),
            None => (LineKind::Context, ""),
            // "\ No newline at end of file" and anything unexpected.
            _ => continue,
        };
        let (old_line, new_line) = match kind {
            LineKind::Added => {
                new_no += 1;
                file.additions += 1;
                (None, Some(new_no - 1))
            }
            LineKind::Removed => {
                old_no += 1;
                file.deletions += 1;
                (Some(old_no - 1), None)
            }
            LineKind::Context => {
                old_no += 1;
                new_no += 1;
                (Some(old_no - 1), Some(new_no - 1))
            }
        };
        hunk.lines.push(DiffLine {
            kind,
            old_line,
            new_line,
            text: text.to_owned(),
        });
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"diff --git a/citas/servicio.py b/citas/servicio.py
index 1111111..2222222 100644
--- a/citas/servicio.py
+++ b/citas/servicio.py
@@ -12,4 +12,5 @@ class Servicio:
 def crear_cita(cliente, servicio, hora):
     cita = Cita.nueva(cliente, servicio, hora)
-    cita.guardar()
+    cita.guardar()
+    programar_recordatorio(cita, horas=24)
     return cita
\ No newline at end of file
diff --git a/tests/test_recordatorios.py b/tests/test_recordatorios.py
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/tests/test_recordatorios.py
@@ -0,0 +1,2 @@
+def test_recordatorio():
+    assert True
diff --git a/viejo.txt b/nuevo.txt
similarity index 100%
rename from viejo.txt
rename to nuevo.txt
diff --git a/logo.png b/logo.png
new file mode 100644
index 0000000..4444444
Binary files /dev/null and b/logo.png differ
"#;

    #[test]
    fn parses_files_hunks_and_line_numbers() {
        let files = parse_unified(SAMPLE);
        assert_eq!(files.len(), 4);

        let servicio = &files[0];
        assert_eq!(servicio.path, "citas/servicio.py");
        assert_eq!(servicio.status, FileStatus::Modified);
        assert_eq!((servicio.additions, servicio.deletions), (2, 1));
        let lines = &servicio.hunks[0].lines;
        assert_eq!(lines.len(), 6);
        assert_eq!((lines[0].old_line, lines[0].new_line), (Some(12), Some(12)));
        assert_eq!(
            (lines[2].kind, lines[2].old_line, lines[2].new_line),
            (LineKind::Removed, Some(14), None)
        );
        assert_eq!(
            (lines[3].kind, lines[3].new_line),
            (LineKind::Added, Some(14))
        );
        assert_eq!(lines[4].text, "    programar_recordatorio(cita, horas=24)");
        assert_eq!((lines[5].old_line, lines[5].new_line), (Some(15), Some(16)));

        let test = &files[1];
        assert_eq!(
            (test.status, test.additions, test.deletions),
            (FileStatus::Added, 2, 0)
        );
        assert_eq!(test.hunks[0].lines[0].new_line, Some(1));

        let renamed = &files[2];
        assert_eq!(renamed.status, FileStatus::Renamed);
        assert_eq!(renamed.path, "nuevo.txt");
        assert_eq!(renamed.old_path.as_deref(), Some("viejo.txt"));

        assert!(files[3].binary);
        assert_eq!(files[3].status, FileStatus::Added);
    }

    #[test]
    fn empty_diff() {
        assert!(parse_unified("").is_empty());
    }
}
