// Hides the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use hedy_core::adapters::hook;

fn main() {
    // Claude Code runs this same binary as its PreToolUse hook.
    if std::env::args().skip(1).eq(hook::HOOK_ARGS) {
        std::process::exit(hook::run());
    }
    hedy_app_lib::run();
}
