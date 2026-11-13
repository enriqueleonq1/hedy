//! Hedy desktop app: a driving adapter over `hedy-core`, like the CLI.

mod commands;
mod dto;
mod terminal;

use hedy_core::adapters::{self, HedyPaths};
use hedy_core::application::Hedy;

/// Shared by every command.
pub struct AppState {
    pub hedy: Hedy,
    pub paths: HedyPaths,
    pub terminals: terminal::Terminals,
}

pub fn run() {
    let paths = HedyPaths::detect().expect("no se encontró la carpeta de datos de Hedy");
    let hedy = adapters::system(&paths).expect("no se pudo abrir la base de datos de Hedy");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            hedy,
            paths,
            terminals: terminal::Terminals::default(),
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
        ])
        .run(tauri::generate_context!())
        .expect("error al ejecutar Hedy");
}
