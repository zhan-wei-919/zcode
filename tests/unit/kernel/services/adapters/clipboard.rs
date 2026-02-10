use super::*;

#[test]
fn test_clipboard_service_creation() {
    let service = ClipboardService::new();
    assert_eq!(service.name(), "ClipboardService");
}

#[cfg(target_os = "linux")]
#[test]
fn wl_paste_uses_no_newline_flag() {
    let cmd = linux::wl_paste_command();
    let args: Vec<String> = cmd
        .get_args()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect();

    assert_eq!(args, vec!["--no-newline"]);
}
