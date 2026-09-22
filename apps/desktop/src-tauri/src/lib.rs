mod advisor;
mod app_updates;
mod backup;
mod connectors;
mod dashboard;
mod document_drop;
mod document_import;
mod document_processing;
mod finance_intelligence;
mod ingestion;
mod onboarding;
mod operations;
mod pilot;
mod reports;
mod review;
mod settings;
mod watched_folder;
mod workspace_database;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(backup::BackupGate::default())
        .manage(app_updates::UpdateGate::default())
        .manage(connectors::ConnectorGate::default())
        .manage(operations::OperationsGate::default())
        .manage(document_processing::WorkerGate::default())
        .manage(document_drop::DocumentDropGate::default())
        .manage(ingestion::IngestionGate::default())
        .manage(watched_folder::WatchedFolderGate::default())
        .manage(pilot::SupportBundleGate::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            advisor::build_ai_consent_preflight,
            advisor::delete_ai_analysis_history,
            advisor::export_ai_analysis_history,
            advisor::list_ai_analysis_history,
            advisor::prepare_ai_analysis,
            advisor::save_ai_analysis_answer,
            app_updates::check_for_update,
            app_updates::get_update_status,
            app_updates::install_update,
            backup::configure_backup_reminder,
            backup::create_workspace_backup,
            backup::export_workspace_portable_data,
            backup::get_backup_status,
            backup::restore_workspace_backup,
            backup::verify_workspace_backup,
            connectors::choose_connector_source,
            connectors::commit_connector_import,
            connectors::export_connector_csv,
            connectors::get_connector_workspace,
            connectors::preview_connector_source,
            dashboard::act_on_rule_event,
            dashboard::get_dashboard,
            dashboard::run_workspace_rules,
            dashboard::save_dashboard_filters,
            dashboard::update_rule_definition,
            document_import::choose_and_import_documents,
            document_drop::configure_document_drop_target,
            document_processing::get_document_worker_status,
            document_processing::process_managed_document,
            finance_intelligence::get_finance_dashboard,
            finance_intelligence::save_finance_dashboard_view,
            finance_intelligence::save_supplier_finance_profile,
            ingestion::cancel_ingestion_batch,
            ingestion::create_manual_extraction,
            ingestion::list_ingestion_batches,
            ingestion::list_supplier_templates,
            ingestion::list_supplier_template_candidates,
            ingestion::pause_ingestion_batch,
            ingestion::recover_ingestion_batches,
            ingestion::retry_ingestion_item,
            ingestion::resolve_probable_duplicate,
            ingestion::propose_supplier_template,
            ingestion::set_supplier_template_status,
            ingestion::start_ingestion_batch,
            onboarding::create_sample_workspace,
            onboarding::delete_sample_workspace,
            operations::commit_bulk_category,
            operations::commit_settlement_import,
            operations::create_operational_category,
            operations::get_operations_workspace,
            operations::choose_settlement_source,
            operations::merge_operational_parties,
            operations::preview_bulk_category,
            operations::record_cash_snapshot,
            operations::record_obligation_event,
            operations::save_operational_party,
            operations::trust_vendor_bank,
            operations::undo_bulk_category,
            operations::update_operational_invoice,
            pilot::export_support_bundle,
            pilot::get_pilot_dashboard,
            pilot::prepare_support_bundle,
            review::add_review_comment,
            review::correct_review_field,
            review::decide_review_case,
            review::get_review_case,
            review::get_review_page_preview,
            review::list_review_queue,
            reports::export_report,
            reports::generate_report_preview,
            reports::get_report_run,
            reports::get_report_workspace,
            reports::save_report_template,
            reports::save_report_reminder,
            settings::get_local_settings,
            settings::save_local_settings,
            watched_folder::choose_and_enable_watched_folder,
            watched_folder::disable_watched_folder,
            watched_folder::get_watched_folder_status,
            workspace_database::create_local_workspace,
            workspace_database::create_local_user,
            workspace_database::get_local_workspace,
            workspace_database::list_local_users,
            workspace_database::list_local_workspaces
        ])
        .on_window_event(document_drop::handle_window_event)
        .run(tauri::generate_context!())
        .expect("AIFOS desktop runtime failed");
}
