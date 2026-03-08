use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub fn send_notification(app: &AppHandle, title: &str, body: &str, sound: bool) {
    let mut builder = app.notification().builder().title(title).body(body);
    if sound {
        builder = builder.sound("default");
    }
    let _ = builder.show();
}

pub fn notify_pipeline_done(app: &AppHandle, issue_number: u64, issue_title: &str, sound: bool) {
    send_notification(
        app,
        "Pipeline Completed",
        &format!("Issue #{} completed - {}", issue_number, issue_title),
        sound,
    );
}

pub fn notify_pipeline_failed(app: &AppHandle, issue_number: u64, stage: &str, sound: bool) {
    send_notification(
        app,
        "Pipeline Failed",
        &format!("Issue #{} failed at {} stage", issue_number, stage),
        sound,
    );
}

pub fn notify_awaiting_approval(app: &AppHandle, issue_number: u64, stage: &str, sound: bool) {
    send_notification(
        app,
        "Approval Required",
        &format!(
            "Issue #{} completed {} stage - awaiting your approval to continue",
            issue_number, stage
        ),
        sound,
    );
}

pub fn notify_all_processed(app: &AppHandle, sound: bool) {
    send_notification(
        app,
        "All Issues Processed",
        "All issues have been processed",
        sound,
    );
}
