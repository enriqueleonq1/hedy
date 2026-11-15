//! Hedy desktop app: a driving adapter over `hedy-core`, like the CLI.

mod commands;
mod dto;
mod events;
mod terminal;
mod waits;

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
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            events::open_tasks_from_notifications(app.handle());
            let events = Arc::new(events::UiEvents(app.handle().clone()));
            let hedy = hedy.with_task_events(events);
            waits::start(&hedy);
            app.manage(AppState {
                hedy,
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
            commands::rename_account,
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
            commands::default_project_parent,
            commands::new_project_path,
            commands::new_project,
            commands::start_conversation,
            commands::conversations,
            commands::conversation_accounts,
            commands::delete_conversation,
            commands::tasks,
            commands::activity,
            commands::task_events,
            commands::answer_permission,
            commands::answer_plan,
            commands::send_message,
            commands::queued,
            commands::queue_message,
            commands::image_support,
            commands::attachment,
            commands::edit_queued,
            commands::remove_queued,
            commands::send_queued_now,
            commands::templates,
            commands::all_templates,
            commands::add_template,
            commands::update_template,
            commands::remove_template,
            commands::restore_builtin_templates,
            commands::stop_task,
            commands::task_diff,
            commands::accept_preview,
            commands::accept_task,
            commands::undo_accept,
            commands::wait_for_limit,
            commands::cancel_wait,
            commands::resume_after_limit,
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
            commands::replace_note,
            commands::forget_note,
            commands::keep_note,
            commands::dismiss_contradiction,
            commands::set_memory_reader,
            commands::skills,
            commands::add_skill,
            commands::edit_skill,
            commands::approve_skill,
            commands::reject_skill,
            commands::remove_skill,
            commands::project_status,
            commands::project_usage,
            commands::usage_summary,
            commands::refresh_plan_usage,
            commands::set_project_client,
            commands::detect_clis,
            commands::doctor,
            commands::open_link,
        ])
        .run(tauri::generate_context!())
        .expect("error al ejecutar Hedy");
}
