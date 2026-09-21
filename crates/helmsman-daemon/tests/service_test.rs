use grub_distro_adapter::{DistroFamily, DistroProfile, FirmwareType};
use helmsman_daemon::{GrubService, TransactionOptions};
use std::fs;
use std::path::PathBuf;

fn get_service_test_dir(name: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join("grub_daemon_test").join(name);
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let config_file = base.join("default_grub");
    let backup_dir = base.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();
    (config_file, backup_dir)
}

#[test]
fn test_service_apply_and_rollback() {
    let (config_file, backup_dir) = get_service_test_dir("apply_rollback");

    // 初始配置
    fs::write(&config_file, "GRUB_DEFAULT=0\nGRUB_TIMEOUT=0\n").unwrap();

    let distro_profile = DistroProfile {
        family: DistroFamily::DebianUbuntu,
        name: "Test Ubuntu".to_string(),
        firmware: FirmwareType::Uefi,
        config_path: "/boot/grub/grub.cfg".to_string(),
        update_command: "true".to_string(),
        command_args: Vec::new(),
    };

    let service =
        GrubService::new_with_paths(config_file.clone(), backup_dir.clone(), distro_profile);

    // 1. 验证 Diff 预览
    let new_config = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=10\n";
    let diff = service.preview_diff(new_config).unwrap();
    assert!(diff.has_changes);
    assert!(diff.diff_text.contains("-GRUB_TIMEOUT=0"));
    assert!(diff.diff_text.contains("+GRUB_TIMEOUT=10"));

    // 2. 提交事务（跳过命令执行）
    let options = TransactionOptions {
        skip_command_execution: true,
    };
    let result = service
        .apply_changes(new_config, "测试修改倒计时", &options)
        .unwrap();
    assert!(result.success);

    // 验证新文件已更新
    let updated_content = fs::read_to_string(&config_file).unwrap();
    assert_eq!(updated_content, new_config);

    // 3. 列出快照
    let snapshots = service.get_available_snapshots().unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id, result.snapshot_id);

    // 4. 回滚快照
    service.rollback_to_snapshot(&result.snapshot_id).unwrap();
    let restored_content = fs::read_to_string(&config_file).unwrap();
    assert_eq!(restored_content, "GRUB_DEFAULT=0\nGRUB_TIMEOUT=0\n");
}

#[test]
fn test_service_package_manager_lock_blocks_apply() {
    use grub_transaction_engine::{LockDescriptor, PackageManagerType};
    use helmsman_daemon::DaemonError;

    let (config_file, backup_dir) = get_service_test_dir("lock_block");
    fs::write(&config_file, "GRUB_DEFAULT=0\n").unwrap();

    let fake_lock = backup_dir.join("pacman_fake.lck");
    fs::write(&fake_lock, "busy").unwrap();

    let distro_profile = DistroProfile {
        family: DistroFamily::Arch,
        name: "Test Arch".to_string(),
        firmware: FirmwareType::Uefi,
        config_path: "/boot/grub/grub.cfg".to_string(),
        update_command: "true".to_string(),
        command_args: Vec::new(),
    };

    let service = GrubService::new_with_paths(config_file, backup_dir, distro_profile)
        .with_lock_descriptors(vec![LockDescriptor::existence(
            PackageManagerType::Pacman,
            &fake_lock,
        )]);

    let options = TransactionOptions::default();
    let res = service.apply_changes("GRUB_DEFAULT=1\n", "测试被锁拦截", &options);

    assert!(res.is_err());
    match res {
        Err(DaemonError::PackageManagerLocked { message }) => {
            assert!(message.contains("pacman"));
        }
        _ => panic!("预期返回 PackageManagerLocked 错误"),
    }
}
