// Hides the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use hedy_core::adapters::{MEMORY_SERVER_ARG, hook};

fn main() {
    // Claude Code runs this same binary as its PreToolUse hook…
    if std::env::args().skip(1).eq(hook::HOOK_ARGS) {
        std::process::exit(hook::run());
    }
    // …and every CLI agent as the project-memory MCP server.
    if std::env::args().nth(1).as_deref() == Some(MEMORY_SERVER_ARG) {
        let served =
            hedy_memory::Options::parse(std::env::args().skip(2)).and_then(hedy_memory::serve);
        if let Err(e) = served {
            eprintln!("hedy-memory: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    hedy_app_lib::run();
}
