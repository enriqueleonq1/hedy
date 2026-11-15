//! Limit waits while the app runs: when an account's limit renews, the core
//! resumes the tasks that waited for it, on the same account (rule 3).

use std::time::Duration;

use hedy_core::application::Hedy;

/// How often due waits are checked. A minute's countdown shows seconds, so
/// the task resumes at most this long after the reset.
const EVERY: Duration = Duration::from_secs(15);

pub fn start(hedy: &Hedy) {
    // A reset that passed while Hedy was closed is not acted on: the task
    // shows "Ya puedes continuar" and the user resumes it.
    let _ = hedy.mark_overdue_waits();
    let hedy = hedy.clone();
    let _ = std::thread::Builder::new()
        .name("hedy-limit-waits".into())
        .spawn(move || {
            loop {
                std::thread::sleep(EVERY);
                let _ = hedy.resume_due_waits();
            }
        });
}
