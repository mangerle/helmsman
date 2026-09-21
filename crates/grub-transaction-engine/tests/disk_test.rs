use grub_transaction_engine::{DiskSpaceError, check_disk_space};
use std::path::PathBuf;

#[test]
fn test_disk_space_display_formatting() {
    let err = DiskSpaceError::InsufficientSpace {
        path: PathBuf::from("/boot"),
        available_bytes: 5 * 1024 * 1024,
        required_bytes: 10 * 1024 * 1024,
    };

    let msg = format!("{}", err);
    assert!(msg.contains("/boot"));
    assert!(msg.contains("5 MB"));
    assert!(msg.contains("10 MB"));
    assert!(msg.contains("可用空间不足"));
}

#[test]
fn test_disk_space_query_failed_formatting() {
    let err = DiskSpaceError::QueryFailed {
        path: PathBuf::from("/boot/efi"),
        reason: "权限不足".to_string(),
    };

    let msg = format!("{}", err);
    assert!(msg.contains("/boot/efi"));
    assert!(msg.contains("权限不足"));
    assert!(msg.contains("查询目标路径可用磁盘空间失败"));
}

#[test]
fn test_check_disk_space_real_directory() {
    let temp_dir = std::env::temp_dir();
    // 检查临时目录是否有至少 1KB 空间，应该必定满足
    let result = check_disk_space(&temp_dir, 1024);
    assert!(result.is_ok());
}
