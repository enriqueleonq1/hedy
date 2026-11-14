// Hides the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Claude Code runs this same binary as its PreToolUse hook, and every CLI
    // agent as the project-memory MCP server.
    if let Some(code) = hedy_memory::serve_helper_mode() {
        std::process::exit(code);
    }
    hedy_app_lib::run();
}
