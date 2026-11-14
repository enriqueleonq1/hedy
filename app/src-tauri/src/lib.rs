//! Hedy desktop app: a driving adapter over `hedy-core`, like the CLI.

mod commands;
mod dto;
mod events;
mod terminal;

use std::sync::Arc;

use hedy_core::adapters::{self, HedyPaths};
use hedy_core::application::Hedy;
use tauri::Manager;

/// Shared by every command.
pub struct AppState {
    pub hedy: Hedy,
    pub paths: HedyPaths,
    pub terminals: Arc<terminal::Terminals>,
}

pub fn run() {
    let paths = HedyPaths::detect().expect("no se encontró la carpeta de datos de Hedy");
    // This binary also serves Claude Code's PreToolUse hook and the memory MCP server (see main.rs).
    let program = std::env::current_exe().expect("no se encontró el ejecutable de Hedy");
    let hedy = adapters::system_for_app(&paths, &program)
        .expect("no se pudo abrir la base de datos de Hedy");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let events = Arc::new(events::UiEvents(app.handle().clone()));
            app.manage(AppState {
                hedy: hedy.with_task_events(events),
                paths,
                terminals: Arc::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::overview,
            commands::add_account,
            commands::set_api_key,
            commands::remove_account,
            commands::set_default_account,
            commands::set_account_color,
            commands::use_account,
            commands::offerable_accounts,
            commands::set_project_color,
            commands::remove_project,
            commands::set_setting,
            commands::open_in_editor,
            commands::terminal_open,
            commands::terminal_write,
            commands::terminal_resize,
            commands::terminal_close,
            commands::start_task,
            commands::tasks,
            commands::task_events,
            commands::answer_permission,
            commands::answer_plan,
            commands::send_message,
            commands::stop_task,
            commands::task_diff,
            commands::accept_task,
            commands::discard_task,
            commands::recent_commits,
            commands::set_project_phases,
            commands::handoff_options,
            commands::handoff_task,
            commands::memory,
            commands::search_memory,
            commands::add_note,
            commands::edit_note,
            commands::approve_note,
            commands::forget_note,
            commands::keep_note,
            commands::dismiss_contradiction,
            commands::set_memory_reader,
            commands::project_status,
            commands::project_usage,
            commands::usage_summary,
            commands::set_project_client,
            commands::detect_clis,
            commands::doctor,
            commands::open_link,
        ])
        .run(tauri::generate_context!())
        .expect("error al ejecutar Hedy");
}
