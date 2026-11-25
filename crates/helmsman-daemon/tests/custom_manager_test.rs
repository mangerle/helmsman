use grub_boot_reader::{CustomBootEntry, generate_custom_script};
use grub_distro_adapter::{DistroFamily, DistroProfile, FirmwareType};
use helmsman_daemon::{CustomManager, DaemonError, GrubService, TransactionOptions};
use std::fs;
use std::path::PathBuf;

fn get_custom_test_env(name: &str) -> (PathBuf, CustomManager, GrubService) {
    let base = std::env::temp_dir().join("helmsman_custom_test").join(name);
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let custom_script = base.join("41_helmsman_custom");
    let aliases_file = base.join("aliases.json");
    let backup_dir = base.join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let default_config = base.join("default_grub");
    fs::write(&default_config, "GRUB_DEFAULT=0\n").unwrap();

    let distro_profile = DistroProfile {
        family: DistroFamily::DebianUbuntu,
        name: "Test Distro".to_string(),
        firmware: FirmwareType::Uefi,
        config_path: "/boot/grub/grub.cfg".to_string(),
        update_command: "true".to_string(),
        command_args: Vec::new(),
        check_command: "true".to_string(),
        check_command_args: Vec::new(),
        grubenv_path: "/boot/grub/grubenv".to_string(),
        set_default_command: "true".to_string(),
    };

    let service = GrubService::new_with_paths(default_config, backup_dir.clone(), distro_profile);
    let manager = CustomManager::new_with_paths(custom_script, aliases_file, backup_dir);
    (base, manager, service)
}

#[test]
fn test_custom_manager_lifecycle() {
    let (_base, manager, service) = get_custom_test_env("lifecycle");

    let entries = manager.load_custom_entries().unwrap();
    assert!(entries.is_empty());

    let item1 = CustomBootEntry::new_iso_boot(
        "iso_1",
        "Ubuntu Live ISO",
        "/boot/iso/ubuntu.iso",
        "UUID-1234",
        "",
    );
    let options = TransactionOptions {
        skip_command_execution: true,
        ..Default::default()
    };
    let res = manager
        .save_custom_entries(&service, &[item1], "测试保存 ISO", &options)
        .unwrap();
    assert!(res.success);

    let reloaded = manager.load_custom_entries().unwrap();
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].id, "iso_1");
    assert_eq!(reloaded[0].title, "Ubuntu Live ISO");

    manager
        .set_alias("gnulinux-6.8", "Ubuntu 6.8 (生产)")
        .unwrap();
    let aliases = manager.load_aliases();
    assert_eq!(aliases.get("gnulinux-6.8").unwrap(), "Ubuntu 6.8 (生产)");

    manager.set_alias("gnulinux-6.8", "").unwrap();
    let aliases_cleared = manager.load_aliases();
    assert!(!aliases_cleared.contains_key("gnulinux-6.8"));
}

#[test]
fn test_delete_custom_entry_only() {
    let (_base, manager, service) = get_custom_test_env("delete_custom_only");

    let options = TransactionOptions {
        skip_command_execution: true,
        ..Default::default()
    };
    let a = CustomBootEntry::new_iso_boot("iso_a", "条目 A", "/iso/a.iso", "uuid-a", "");
    let b = CustomBootEntry::new_iso_boot("iso_b", "条目 B", "/iso/b.iso", "uuid-b", "");
    manager
        .save_custom_entries(&service, &[a, b], "写入两项", &options)
        .unwrap();
    assert_eq!(manager.load_custom_entries().unwrap().len(), 2);

    let res = manager
        .delete_custom_entry(&service, "iso_a", "删除 A", &options)
        .unwrap();
    assert!(res.success);
    let left = manager.load_custom_entries().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, "iso_b");

    let err = manager
        .delete_custom_entry(&service, "gnulinux-simple-xxx", "尝试删系统项", &options)
        .unwrap_err();
    assert!(matches!(err, DaemonError::CustomEntryInvalid { .. }));
    assert_eq!(manager.load_custom_entries().unwrap().len(), 1);
}

#[test]
fn test_alias_is_display_only_and_validated() {
    let (_base, manager, _service) = get_custom_test_env("alias_display_only");

    let entry =
        CustomBootEntry::new_iso_boot("iso_alias", "原始标题", "/boot/iso/a.iso", "UUID-A", "");
    let script_before = generate_custom_script(std::slice::from_ref(&entry)).unwrap();

    manager.set_alias("iso_alias", "短名").unwrap();
    let script_after = generate_custom_script(std::slice::from_ref(&entry)).unwrap();
    assert_eq!(script_before, script_after);
    assert!(script_after.contains("menuentry '原始标题'"));
    assert!(!script_after.contains("短名"));

    assert!(matches!(
        manager.set_alias("iso_alias", "bad\nname"),
        Err(DaemonError::InvalidAlias { .. })
    ));
    assert!(matches!(
        manager.set_alias("", "合法名"),
        Err(DaemonError::InvalidAlias { .. })
    ));
    let long = "长".repeat(129);
    assert!(matches!(
        manager.set_alias("iso_alias", &long),
        Err(DaemonError::InvalidAlias { .. })
    ));
}

#[test]
fn test_custom_manager_rejects_conflicts() {
    let (_base, manager, service) = get_custom_test_env("conflict");

    let a = CustomBootEntry::new_iso_boot("dup_id", "条目 A", "/boot/iso/a.iso", "UUID-A", "");
    let b = CustomBootEntry::new_iso_boot("dup_id", "条目 B", "/boot/iso/b.iso", "UUID-B", "");
    let options = TransactionOptions {
        skip_command_execution: true,
        ..Default::default()
    };
    let err = manager
        .save_custom_entries(&service, &[a, b], "ID 冲突", &options)
        .unwrap_err();
    assert!(matches!(err, DaemonError::CustomEntryInvalid { .. }));

    let c = CustomBootEntry::new_iso_boot("id_c", "同名", "/boot/iso/c.iso", "UUID-C", "");
    let d = CustomBootEntry::new_iso_boot("id_d", "同名", "/boot/iso/d.iso", "UUID-D", "");
    let err2 = manager
        .save_custom_entries(&service, &[c, d], "标题冲突", &options)
        .unwrap_err();
    assert!(matches!(err2, DaemonError::CustomEntryInvalid { .. }));

    assert!(manager.load_custom_entries().unwrap().is_empty());
}

#[test]
fn test_custom_manager_rollback_on_update_failure() {
    let (_base, manager, service) = get_custom_test_env("rollback_custom");

    let options = TransactionOptions {
        skip_command_execution: true,
        ..Default::default()
    };
    let first =
        CustomBootEntry::new_iso_boot("iso_ok", "稳定条目", "/boot/iso/ok.iso", "UUID-OK", "");
    manager
        .save_custom_entries(&service, &[first], "首次写入", &options)
        .unwrap();
    let before = fs::read_to_string(&manager.custom_script_path).unwrap();

    let failing_profile = DistroProfile {
        update_command: "false".to_string(),
        ..service.distro_profile.clone()
    };
    let failing_service = GrubService::new_with_paths(
        service.default_config_path.clone(),
        service.backup_dir.clone(),
        failing_profile,
    );

    let second = CustomBootEntry::new_iso_boot(
        "iso_fail",
        "将回滚条目",
        "/boot/iso/fail.iso",
        "UUID-FAIL",
        "",
    );
    let options_run = TransactionOptions {
        skip_command_execution: false,
        skip_syntax_check: true,
        ..Default::default()
    };
    let err = manager
        .save_custom_entries(
            &failing_service,
            &[second],
            "更新失败应整体回滚",
            &options_run,
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            DaemonError::CommandLaunchFailed { .. } | DaemonError::SecurityCheckFailed(_)
        ),
        "实际错误: {:?}",
        err
    );

    let after = fs::read_to_string(&manager.custom_script_path).unwrap();
    assert_eq!(before, after);
    assert!(!after.contains("将回滚条目"));
}
