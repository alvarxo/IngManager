mod codes;
mod commands;
mod db;
#[cfg(test)]
mod db_tests;
mod error;
mod models;
mod path_security;
mod state;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let db_path = db::default_db_path().expect("data dir");
            let pool = tauri::async_runtime::block_on(db::init_pool(&db_path))
                .expect("Failed to initialize SQLite database");
            app.manage(AppState::new(pool, db_path));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::settings::get_app_status,
            commands::settings::set_root_path,
            commands::settings::get_root_path,
            commands::projects::preview_next_project_code,
            commands::projects::list_projects,
            commands::projects::get_project,
            commands::projects::create_project,
            commands::projects::update_project,
            commands::projects::archive_project,
            commands::projects::open_project,
            commands::projects::list_project_categories,
            commands::projects::list_recent_projects,
            commands::projects::list_favorite_projects,
            commands::files::list_category_files,
            commands::files::open_file,
            commands::files::reveal_in_explorer,
            commands::files::open_project_folder,
            commands::indexing::start_initial_index,
            commands::indexing::reindex_root,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
