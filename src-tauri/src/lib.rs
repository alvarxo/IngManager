mod audit;
mod codes;
mod commands;
mod db;
#[cfg(test)]
mod db_tests;
mod doc_codes;
mod error;
#[cfg(test)]
mod fs_ops_tests;
mod fs_util;
mod index_sync;
mod models;
mod path_security;
mod search_index;
mod state;
mod watcher;
mod zip_safe;

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
            commands::documents::list_documents,
            commands::documents::get_document,
            commands::documents::preview_next_document_code,
            commands::documents::create_document,
            commands::documents::update_document,
            commands::documents::attach_file_to_version,
            commands::documents::list_document_versions,
            commands::documents::list_version_files,
            commands::documents::create_document_version,
            commands::documents::restore_document_version,
            commands::documents::open_document_file,
            commands::documents::list_revisions,
            commands::documents::create_revision,
            commands::documents::list_subcategories,
            commands::documents::create_subcategory,
            commands::documents::list_document_relationships,
            commands::documents::create_document_relationship,
            commands::documents::delete_document_relationship,
            commands::documents::set_document_favorite,
            commands::documents::list_favorite_documents,
            commands::documents::is_document_favorite,
            commands::search::global_search,
            commands::search::rebuild_search_index,
            commands::audit_cmd::list_project_audit,
            commands::backup::create_portable_backup,
            commands::backup::list_backup_candidates,
            commands::import_wizard::detect_importable_projects,
            commands::import_wizard::preview_import_project,
            commands::import_wizard::import_existing_project,
            commands::import_wizard::propose_import_code,
            commands::transmissions::list_transmissions,
            commands::transmissions::get_transmission,
            commands::transmissions::list_transmission_items,
            commands::transmissions::preview_next_transmission_code,
            commands::transmissions::preview_sent_transmission,
            commands::transmissions::create_sent_transmission,
            commands::transmissions::import_received_transmission,
            commands::transmissions::link_transmissions,
            commands::transmissions::list_transmission_links,
            commands::transmissions::open_transmission_folder,
            commands::file_manager::list_directory,
            commands::file_manager::resolve_project_category_path,
            commands::file_manager::create_folder,
            commands::file_manager::rename_entry,
            commands::file_manager::copy_entries,
            commands::file_manager::move_entries,
            commands::file_manager::import_files,
            commands::file_manager::delete_to_trash,
            commands::file_manager::list_trash,
            commands::file_manager::restore_trash_item,
            commands::file_manager::purge_trash_item,
            commands::file_manager::get_entry_properties,
            commands::file_manager::open_path,
            commands::watcher_cmd::start_file_watcher,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
