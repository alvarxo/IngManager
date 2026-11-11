mod audit;
mod auto_version;
mod category_mapping;
mod codes;
mod commands;
mod db;
#[cfg(test)]
mod db_tests;
mod doc_codes;
mod doc_revisions;
mod engineering_files;
#[cfg(test)]
mod engineering_sap_tests;
#[cfg(test)]
mod models_versions_hotfix_tests;
#[cfg(test)]
mod usability_versions_hotfix_tests;
#[cfg(test)]
mod memory_revisions_hotfix_tests;
#[cfg(test)]
mod revision_folders_tests;
#[cfg(test)]
mod model_identity_hotfix_tests;
#[cfg(test)]
mod remove_project_hotfix_tests;
mod error;
#[cfg(test)]
mod flexibility_tests;
#[cfg(test)]
mod fs_ops_tests;
mod fs_util;
mod inbox;
mod incorporation;
#[cfg(test)]
mod incorporation_tests;
mod index_sync;
mod logical_tree;
#[cfg(test)]
mod logical_tree_nav_tests;
mod model_spreadsheets;
#[cfg(test)]
mod model_spreadsheets_tests;
mod model_types;
#[cfg(test)]
mod model_subtypes_codes_tests;
mod derived_models;
mod model_revisions;
#[cfg(test)]
mod model_revision_parity_tests;
#[cfg(test)]
mod derived_models_tests;
mod models;
mod unify_models;
#[cfg(test)]
mod unify_models_tests;
mod repair_model_history;
#[cfg(test)]
mod sap_false_versions_hotfix_tests;
#[cfg(test)]
mod organizer_inbox_tests;
mod organizer;
mod revision_folders;
mod path_security;
mod project_paths;
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
            commands::settings::list_authorized_roots,
            commands::settings::add_authorized_root,
            commands::settings::remove_authorized_root,
            commands::settings::update_authorized_root_label,
            commands::projects::get_project_absolute_path,
            commands::projects::preview_next_project_code,
            commands::projects::list_projects,
            commands::projects::get_project,
            commands::projects::create_project,
            commands::projects::update_project,
            commands::projects::archive_project,
            commands::projects::open_project,
            commands::projects::remove_project_registration,
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
            commands::documents::update_document_code_from_model_type,
            commands::documents::attach_file_to_version,
            commands::documents::associate_model_spreadsheet,
            commands::documents::detach_model_spreadsheet,
            commands::documents::model_spreadsheet_status,
            commands::incorporation_cmd::preview_document_incorporation,
            commands::incorporation_cmd::apply_document_incorporation,
            commands::documents::list_document_versions,
            commands::documents::list_version_files,
            commands::documents::create_document_version,
            commands::documents::preview_next_physical_version,
            commands::documents::create_next_physical_version,
            commands::documents::preview_create_derived_model,
            commands::documents::create_derived_model,
            commands::documents::repair_version_file,
            commands::documents::restore_document_version,
            commands::documents::delete_logical_document,
            commands::documents::cleanup_document_availability,
            commands::documents::open_document_file,
            commands::documents::open_document_principal,
            commands::documents::list_document_open_formats,
            commands::documents::get_document_open_preference,
            commands::documents::set_document_open_preference,
            commands::documents::open_document_with_format,
            commands::documents::list_revision_files,
            commands::documents::list_revisions,
            commands::settings::get_app_setting,
            commands::settings::set_app_setting,
            commands::settings::get_logical_tree_expand_policy,
            commands::settings::set_logical_tree_expand_policy,
            commands::settings::get_logical_tree_expand_state,
            commands::settings::set_logical_tree_expand_state,
            commands::documents::create_revision,
            commands::documents::repair_document_revisions,
            commands::documents::list_subcategories,
            commands::documents::create_subcategory,
            commands::documents::update_subcategory,
            commands::documents::delete_subcategory,
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
            commands::file_manager::preview_external_drop,
            commands::file_manager::apply_external_drop,
            commands::file_manager::delete_to_trash,
            commands::file_manager::list_trash,
            commands::file_manager::restore_trash_item,
            commands::file_manager::purge_trash_item,
            commands::file_manager::get_entry_properties,
            commands::file_manager::open_path,
            commands::watcher_cmd::start_file_watcher,
            commands::organizer::organizer_scan,
            commands::organizer::organizer_build_plan,
            commands::organizer::organizer_get_plan,
            commands::organizer::organizer_apply_plan,
            commands::organizer::organizer_update_plan_item,
            commands::organizer::organizer_list_sessions,
            commands::organizer::list_project_folder_mappings,
            commands::revision_folders_cmd::preview_revision_folder_reorg,
            commands::revision_folders_cmd::apply_revision_folder_reorg,
            commands::engineering::list_engineering_groups,
            commands::engineering::list_category_files_filtered,
            commands::engineering::bulk_register_engineering_docs,
            commands::engineering::sync_model_versions,
            commands::engineering::associate_model_version,
            commands::engineering::list_unify_model_suggestions,
            commands::engineering::preview_unify_models,
            commands::engineering::apply_unify_models,
            commands::engineering::preview_repair_model_history,
            commands::engineering::apply_repair_model_history,
            commands::inbox_cmd::inbox_ensure_dir,
            commands::inbox_cmd::inbox_list,
            commands::inbox_cmd::inbox_stage_files,
            commands::inbox_cmd::inbox_remove_items,
            commands::inbox_cmd::inbox_assign,
            commands::auto_version_cmd::list_pending_versions,
            commands::auto_version_cmd::resolve_pending_version,
            commands::auto_version_cmd::dismiss_pending_version,
            commands::auto_version_cmd::reconcile_project_versions,
            commands::auto_version_cmd::register_file_version_baseline,
            commands::auto_version_cmd::process_auto_version_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
