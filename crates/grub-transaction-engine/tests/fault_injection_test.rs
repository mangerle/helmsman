use grub_transaction_engine::{atomic_write, create_snapshot, list_snapshots, restore_snapshot};
use std::fs;
use std::path::PathBuf;

fn get_fault_test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("grub_fault_test").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_atomic_write_preserves_original_on_target_directory_conflict() {
    let test_dir = get_fault_test_dir("atomic_conflict");
    let target_file = test_dir.join("grub_config");
    fs::write(&target_file, "ORIGINAL_CONTENT").unwrap();

    // 制造故障：将一个子目录当做文件路径，导致写入或替换冲突
    let conflict_path = test_dir.join("a_directory_not_a_file");
    fs::create_dir_all(&conflict_path).unwrap();

    let err = atomic_write(&conflict_path, "NEW_CONTENT");
    assert!(err.is_err(), "向目录路径原子写入应该失败");

    // 验证原目标文件完全未受影响
    let original = fs::read_to_string(&target_file).unwrap();
    assert_eq!(original, "ORIGINAL_CONTENT");
}

#[test]
fn test_atomic_write_cleans_up_temp_file_on_error() {
    let test_dir = get_fault_test_dir("temp_cleanup");

    // 创建一个只读文件来触发覆盖失败（在 Windows/Unix 上测试只读文件）
    let target_file = test_dir.join("readonly_target");
    fs::write(&target_file, "READONLY_CONTENT").unwrap();

    let mut perms = fs::metadata(&target_file).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&target_file, perms).unwrap();

    // 尝试原子写入（重命名覆盖只读文件通常会失败）
    let _ = atomic_write(&target_file, "OVERWRITE_ATTEMPT");

    // 恢复写权限以允许后续清理
    let mut writable_perms = fs::metadata(&target_file).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    writable_perms.set_readonly(false);
    let _ = fs::set_permissions(&target_file, writable_perms);

    // 验证目录下没有遗留任何 .tmp_* 前缀的临时文件
    for entry in fs::read_dir(&test_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            !name.starts_with(".tmp_"),
            "发现残留未清理的临时文件: {}",
            name
        );
    }
}

#[test]
fn test_transaction_rollback_after_simulated_crash() {
    let test_dir = get_fault_test_dir("simulated_crash");
    let config_file = test_dir.join("grub");
    let backup_dir = test_dir.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let initial_content = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n";
    fs::write(&config_file, initial_content).unwrap();

    // 1. 创建事务前快照
    let snapshot = create_snapshot(&config_file, &backup_dir, "崩溃前快照").unwrap();

    // 2. 模拟阶段一：配置已写入新内容
    let corrupted_content = "GRUB_DEFAULT=9999\nGRUB_TIMEOUT=-1\n";
    atomic_write(&config_file, corrupted_content).unwrap();
    assert_eq!(fs::read_to_string(&config_file).unwrap(), corrupted_content);

    // 3. 模拟阶段二发生“崩溃”或“下游校验失败”：触发快照回滚
    restore_snapshot(&snapshot).unwrap();

    // 4. 验证完全恢复到初始状态
    let restored_content = fs::read_to_string(&config_file).unwrap();
    assert_eq!(restored_content, initial_content);
}

#[test]
fn test_restore_snapshot_fails_gracefully_when_backup_missing() {
    let test_dir = get_fault_test_dir("missing_backup");
    let config_file = test_dir.join("grub");
    let backup_dir = test_dir.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    fs::write(&config_file, "GRUB_DEFAULT=0\n").unwrap();
    let snapshot = create_snapshot(&config_file, &backup_dir, "即将丢失的快照").unwrap();

    // 人为删除备份文件以模拟存储损坏
    fs::remove_file(&snapshot.backup_file).unwrap();

    // 验证回滚返回 NotFound 错误而非 panic
    let result = restore_snapshot(&snapshot);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn test_list_snapshots_filters_corrupted_metadata() {
    let test_dir = get_fault_test_dir("corrupted_meta");
    let backup_dir = test_dir.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    // 创建一个合法的快照目录
    let valid_dir = backup_dir.join("snapshot_100_1");
    fs::create_dir_all(&valid_dir).unwrap();
    fs::write(
        valid_dir.join("info.txt"),
        "id=snapshot_100_1\ntimestamp=100\nreason=合法\ntarget=/etc/default/grub\nbackup=/tmp/b\n",
    )
    .unwrap();

    // 创建一个缺少 info.txt 的空目录（模拟写入中途断电）
    let incomplete_dir = backup_dir.join("snapshot_200_2");
    fs::create_dir_all(&incomplete_dir).unwrap();

    // 创建一个非法内容的目录
    let garbage_dir = backup_dir.join("snapshot_300_3");
    fs::create_dir_all(&garbage_dir).unwrap();
    fs::write(garbage_dir.join("info.txt"), "corrupted garbage data").unwrap();

    // 验证 list_snapshots 能够安全解析，过滤缺失 info.txt 的目录并容错
    let snapshots = list_snapshots(&backup_dir).unwrap();
    assert!(!snapshots.is_empty());
    assert!(snapshots.iter().any(|s| s.id == "snapshot_100_1"));
}
