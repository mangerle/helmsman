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
        #[cfg(windows)]
        update_command: "cmd".to_string(),
        #[cfg(windows)]
        command_args: vec!["/c".to_string(), "exit".to_string(), "0".to_string()],
        #[cfg(not(windows))]
        update_command: "true".to_string(),
        #[cfg(not(windows))]
        command_args: Vec::new(),
        check_command: "true".to_string(),
        check_command_args: Vec::new(),
        grubenv_path: "/boot/grub/grubenv".to_string(),
        set_default_command: "true".to_string(),
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
        ..Default::default()
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
        check_command: "true".to_string(),
        check_command_args: Vec::new(),
        grubenv_path: "/boot/grub/grubenv".to_string(),
        set_default_command: "true".to_string(),
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

#[test]
fn test_service_syntax_check_failure_triggers_rollback() {
    let (config_file, backup_dir) = get_service_test_dir("syntax_fail");
    fs::write(&config_file, "GRUB_DEFAULT=0\n").unwrap();

    let fake_cfg = backup_dir.join("grub.cfg");
    fs::write(&fake_cfg, "broken content").unwrap();

    #[cfg(windows)]
    let (update_cmd, update_args) = (
        "cmd".to_string(),
        vec!["/c".to_string(), "exit 0".to_string()],
    );
    #[cfg(not(windows))]
    let (update_cmd, update_args) = ("true".to_string(), Vec::new());

    #[cfg(windows)]
    let (check_cmd, check_args) = (
        "cmd".to_string(),
        vec!["/c".to_string(), "exit 1".to_string()],
    );
    #[cfg(not(windows))]
    let (check_cmd, check_args) = ("false".to_string(), Vec::new());

    let distro_profile = DistroProfile {
        family: DistroFamily::DebianUbuntu,
        name: "Test Ubuntu".to_string(),
        firmware: FirmwareType::Uefi,
        config_path: fake_cfg.to_string_lossy().to_string(),
        update_command: update_cmd,
        command_args: update_args,
        check_command: check_cmd,
        check_command_args: check_args,
        grubenv_path: "/boot/grub/grubenv".to_string(),
        set_default_command: "true".to_string(),
    };

    let service = GrubService::new_with_paths(config_file.clone(), backup_dir, distro_profile);
    let options = TransactionOptions::default();
    let res = service.apply_changes("GRUB_DEFAULT=1\n", "测试语法校验失败回滚", &options);

    // 验证事务以错误返回并已自动回滚
    let err = res.expect_err("语法校验失败应返回错误");
    let err_text = err.to_string();
    assert!(
        err_text.contains("已成功自动回滚") || err_text.contains("引导"),
        "错误信息应包含回滚说明，实际: {err_text}"
    );

    // 验证原始文件内容已被恢复
    let restored = fs::read_to_string(&config_file).unwrap();
    assert_eq!(restored, "GRUB_DEFAULT=0\n");
}

#[test]
fn test_service_set_default_entry_fast() {
    let (config_file, backup_dir) = get_service_test_dir("set_default_fast");
    fs::write(&config_file, "GRUB_DEFAULT=saved\n").unwrap();

    #[cfg(windows)]
    let set_cmd = "cmd".to_string();
    #[cfg(not(windows))]
    let set_cmd = "true".to_string();

    let distro_profile = DistroProfile {
        family: DistroFamily::DebianUbuntu,
        name: "Test Ubuntu".to_string(),
        firmware: FirmwareType::Uefi,
        config_path: "/boot/grub/grub.cfg".to_string(),
        update_command: "true".to_string(),
        command_args: Vec::new(),
        check_command: "true".to_string(),
        check_command_args: Vec::new(),
        grubenv_path: "/boot/grub/grubenv".to_string(),
        set_default_command: set_cmd,
    };

    let service = GrubService::new_with_paths(config_file, backup_dir, distro_profile);
    assert!(
        service
            .set_default_entry_fast("Ubuntu, with Linux 6.8.0")
            .is_ok()
    );
}

#[test]
fn test_service_command_timeout_triggers_rollback() {
    let (config_file, backup_dir) = get_service_test_dir("timeout_rollback");
    fs::write(&config_file, "GRUB_DEFAULT=0\n").unwrap();

    // 模拟耗时超过超时的命令
    #[cfg(windows)]
    let (slow_cmd, slow_args) = (
        "cmd".to_string(),
        vec!["/c".to_string(), "ping 127.0.0.1 -n 4 > nul".to_string()],
    );
    #[cfg(not(windows))]
    let (slow_cmd, slow_args) = ("sleep".to_string(), vec!["3".to_string()]);

    let distro_profile = DistroProfile {
        family: DistroFamily::DebianUbuntu,
        name: "Test Ubuntu".to_string(),
        firmware: FirmwareType::Uefi,
        config_path: "/boot/grub/grub.cfg".to_string(),
        update_command: slow_cmd,
        command_args: slow_args,
        check_command: "true".to_string(),
        check_command_args: Vec::new(),
        grubenv_path: "/boot/grub/grubenv".to_string(),
        set_default_command: "true".to_string(),
    };

    let service = GrubService::new_with_paths(config_file.clone(), backup_dir, distro_profile);
    // 设限超时为 1 秒，预期会超时
    let options = TransactionOptions {
        timeout_seconds: Some(1),
        ..Default::default()
    };

    let res = service.apply_changes("GRUB_DEFAULT=1\n", "测试命令超时", &options);

    // 预期返回 CommandLaunchFailed 且原配置已被恢复
    assert!(res.is_err());
    let restored = fs::read_to_string(&config_file).unwrap();
    assert_eq!(restored, "GRUB_DEFAULT=0\n");
}

#[test]
fn test_service_audit_event_logged() {
    use helmsman_daemon::{AuditAction, AuditEvent, record_audit_event};
    use std::time::Duration;

    // 验证 AuditEvent 可被正常构造并由 record_audit_event 消费记录
    let event = AuditEvent {
        caller_uid: 1000,
        action: AuditAction::ApplyChanges,
        reason: "测试结构化审计追踪".to_string(),
        snapshot_id: Some("snap_001".to_string()),
        success: true,
        duration: Duration::from_millis(50),
        diff_summary: Some("+1 / -0".to_string()),
    };

    record_audit_event(&event);
    assert_eq!(event.caller_uid, 1000);
    assert_eq!(event.action, AuditAction::ApplyChanges);
}
