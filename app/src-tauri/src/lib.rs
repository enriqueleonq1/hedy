//! Hedy desktop app: a driving adapter over `hedy-core`, like the CLI.

mod ask;
mod carry;
mod code;
mod commands;
mod dto;
mod events;
mod ship;
mod sync;
mod team;
mod terminal;
mod themes;
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
            sync::start(app.handle().clone(), &hedy);
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
            carry::conversation_context,
            carry::summarize_conversation,
            carry::continuations,
            carry::save_conversation_note,
            commands::tasks,
            commands::activity,
            commands::task_events,
            commands::answer_permission,
            commands::answer_plan,
            commands::send_message,
            commands::queued,
            commands::queue_message,
            commands::image_support,
            commands::project_files,
            commands::attachment,
            commands::edit_queued,
            commands::remove_queued,
            commands::send_queued_now,
            commands::templates,
            commands::account_commands,
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
            ask::ask_memory,
            ask::memory_graph,
            ask::look_in_code,
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
            code::code_copy,
            code::code_tree,
            code::read_file,
            code::file_diff,
            code::search_text,
            code::file_history,
            code::blame_file,
            code::file_symbols,
            code::open_file_in_editor,
            code::reveal_in_file_manager,
            code::write_file,
            code::create_file,
            code::create_folder,
            code::rename_path,
            code::delete_path,
            code::duplicate_file,
            ship::folder_status,
            ship::push_folder,
            ship::pull_folder,
            ship::ship_state,
            ship::push_task,
            ship::pull_request_draft,
            ship::create_pull_request,
            ship::suggest_commit_message,
            themes::list_themes,
            themes::save_theme,
            themes::delete_theme,
            themes::describe_theme,
            sync::sync_status,
            sync::sync_default_name,
            sync::sync_repo_is_public,
            sync::sync_setup,
            sync::sync_join,
            sync::sync_now,
            sync::sync_remove_device,
            sync::sync_leave,
            sync::sync_conflicts,
            sync::resolve_sync_conflict,
            sync::memory_home,
            sync::memory_homes,
            sync::set_memory_home,
            sync::answer_owner,
            sync::forget_owner,
            team::teams,
            team::team_status,
            team::team_create,
            team::team_invite,
            team::team_cancel_invite,
            team::team_dismiss_failed_invite,
            team::team_join,
            team::team_sync,
            team::team_link_project,
            team::team_notes,
            team::team_add_note,
            team::team_edit_note,
            team::team_publish_note,
            team::team_suggestions,
            team::team_accept_suggestion,
            team::team_reject_suggestion,
            team::team_set_role,
            team::team_remove_person,
            team::team_remove_pc,
            team::team_rename,
            team::team_rename_person,
            team::team_leave,
        ])
        .run(tauri::generate_context!())
        .expect("error al ejecutar Hedy");
}
