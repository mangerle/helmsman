use grub_transaction_engine::{
    atomic_write, create_snapshot, generate_unified_diff, list_snapshots, restore_snapshot,
};
use std::fs;
use std::path::PathBuf;

fn get_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("grub_manager_test").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_diff_calculation() {
    let original = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=0\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\"\n";
    let modified =
        "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash nomodeset\"\n";

    let report = generate_unified_diff(original, modified);
    assert!(report.has_changes);
    assert_eq!(report.added_lines, 2);
    assert_eq!(report.removed_lines, 2);
    assert!(report.diff_text.contains("-GRUB_TIMEOUT=0"));
    assert!(report.diff_text.contains("+GRUB_TIMEOUT=5"));
}

#[test]
fn test_atomic_write_lifecycle() {
    let test_dir = get_test_dir("atomic_test");
    let target_file = test_dir.join("grub_test");

    // 首次写入
    atomic_write(&target_file, "KEY1=VALUE1\n").unwrap();
    let content = fs::read_to_string(&target_file).unwrap();
    assert_eq!(content, "KEY1=VALUE1\n");

    // 第二次覆盖写入
    atomic_write(&target_file, "KEY1=VALUE2\n").unwrap();
    let updated = fs::read_to_string(&target_file).unwrap();
    assert_eq!(updated, "KEY1=VALUE2\n");
}

#[test]
fn test_snapshot_and_restore() {
    let test_dir = get_test_dir("snapshot_test");
    let target_file = test_dir.join("default_grub");
    let backup_dir = test_dir.join("backups");

    // 初始配置
    fs::write(&target_file, "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n").unwrap();

    // 创建快照
    let meta = create_snapshot(&target_file, &backup_dir, "测试备份").unwrap();
    assert_eq!(meta.reason, "测试备份");

    // 列出快照检查
    let snapshots = list_snapshots(&backup_dir).unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id, meta.id);

    // 模拟破坏性修改
    fs::write(&target_file, "CORRUPTED CONFIG\n").unwrap();
    assert_eq!(
        fs::read_to_string(&target_file).unwrap(),
        "CORRUPTED CONFIG\n"
    );

    // 执行快照恢复
    restore_snapshot(&meta).unwrap();
    let restored = fs::read_to_string(&target_file).unwrap();
    assert_eq!(restored, "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n");
}
