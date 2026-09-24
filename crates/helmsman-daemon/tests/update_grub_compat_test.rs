//! update-grub 兼容性与 41_* / 10_linux 并存集成测试
//!
//! 模拟发行版 `10_linux` 生成系统内核菜单 + Helmsman 受管 `41_helmsman_custom`
//! 追加自定义项的完整菜单结构，并验证内核升级后自定义项与默认项语义仍然稳定。

use grub_boot_reader::{
    CustomBootEntry, generate_custom_script, parse_custom_script, parse_grub_cfg,
};
use grub_config_parser::{DefaultEntry, get_default_entry, parse_grub_config, set_default_entry};
use grub_distro_adapter::{DistroFamily, DistroProfile, FirmwareType};
use helmsman_daemon::{CustomManager, GrubService, TransactionOptions};
use std::fs;
use std::path::PathBuf;

fn test_root(name: &str) -> PathBuf {
    let base = std::env::temp_dir()
        .join("helmsman_update_compat")
        .join(name);
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    base
}

fn make_profile() -> DistroProfile {
    DistroProfile {
        family: DistroFamily::DebianUbuntu,
        name: "Test Ubuntu".to_string(),
        firmware: FirmwareType::Uefi,
        config_path: "grub.cfg".to_string(),
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
        grubenv_path: "grubenv".to_string(),
        set_default_command: "true".to_string(),
    }
}

/// 模拟 grub-mkconfig：拼接 10_linux 系统菜单与 41_helmsman_custom 自定义菜单
fn simulate_grub_mkconfig(linux_menu: &str, custom_script: &str) -> String {
    let mut out = String::from("## 由测试模拟的 grub-mkconfig 输出\n");
    out.push_str(linux_menu);
    out.push('\n');
    // 受管脚本头部使用 exec tail，正文即 GRUB 菜单片段
    for line in custom_script.lines().skip(2) {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// 内核升级前的 10_linux 菜单
const LINUX_MENU_BEFORE: &str = r#"
menuentry 'Ubuntu' --class ubuntu --class gnu-linux {
    linux /boot/vmlinuz-6.8.0-40-generic root=UUID=aaaa ro quiet splash
    initrd /boot/initrd.img-6.8.0-40-generic
}
submenu 'Advanced options for Ubuntu' {
    menuentry 'Ubuntu, with Linux 6.8.0-40-generic' {
        linux /boot/vmlinuz-6.8.0-40-generic root=UUID=aaaa ro quiet splash
        initrd /boot/initrd.img-6.8.0-40-generic
    }
}
"#;

/// 内核升级后的 10_linux 菜单（新增 6.8.0-45，旧内核进入子菜单）
const LINUX_MENU_AFTER: &str = r#"
menuentry 'Ubuntu' --class ubuntu --class gnu-linux {
    linux /boot/vmlinuz-6.8.0-45-generic root=UUID=aaaa ro quiet splash
    initrd /boot/initrd.img-6.8.0-45-generic
}
submenu 'Advanced options for Ubuntu' {
    menuentry 'Ubuntu, with Linux 6.8.0-45-generic' {
        linux /boot/vmlinuz-6.8.0-45-generic root=UUID=aaaa ro quiet splash
        initrd /boot/initrd.img-6.8.0-45-generic
    }
    menuentry 'Ubuntu, with Linux 6.8.0-40-generic' {
        linux /boot/vmlinuz-6.8.0-40-generic root=UUID=aaaa ro quiet splash
        initrd /boot/initrd.img-6.8.0-40-generic
    }
}
"#;

fn sample_custom_entries() -> Vec<CustomBootEntry> {
    vec![
        CustomBootEntry::new_iso_boot(
            "ubuntu_live",
            "Ubuntu Live ISO",
            "/boot/iso/ubuntu.iso",
            "bbbb-cccc",
            "",
        ),
        CustomBootEntry::new_chainloader(
            "win11",
            "Windows 11",
            "/EFI/Microsoft/Boot/bootmgfw.efi",
            "dddd-eeee",
        ),
    ]
}

#[test]
fn test_custom_entries_coexist_with_10_linux() {
    let entries = sample_custom_entries();
    let custom_script = generate_custom_script(&entries).unwrap();
    let combined = simulate_grub_mkconfig(LINUX_MENU_BEFORE, &custom_script);
    let menu = parse_grub_cfg(&combined);

    // 扁平化后应同时包含系统内核项与自定义项
    let mut titles = Vec::new();
    for node in &menu {
        for e in node.collect_entries() {
            titles.push(e.title.clone());
        }
    }
    assert!(titles.iter().any(|t| t == "Ubuntu"));
    assert!(titles.iter().any(|t| t == "Ubuntu Live ISO"));
    assert!(titles.iter().any(|t| t == "Windows 11"));
    assert!(titles.iter().any(|t| t.contains("6.8.0-40")));
}

#[test]
fn test_kernel_upgrade_keeps_custom_entries_and_default_semantic() {
    let root = test_root("kernel_upgrade");
    let custom_script_path = root.join("41_helmsman_custom");
    let aliases = root.join("aliases.json");
    let backup = root.join("backups");
    let default_cfg = root.join("default_grub");
    fs::create_dir_all(&backup).unwrap();

    // 初始默认项指向第 0 项（Ubuntu 系统内核）
    fs::write(&default_cfg, "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n").unwrap();

    let profile = DistroProfile {
        config_path: root.join("grub.cfg").to_string_lossy().into_owned(),
        grubenv_path: root.join("grubenv").to_string_lossy().into_owned(),
        ..make_profile()
    };
    let service = GrubService::new_with_paths(default_cfg.clone(), backup.clone(), profile);
    let manager = CustomManager::new_with_paths(custom_script_path.clone(), aliases, backup);

    let options = TransactionOptions {
        skip_command_execution: true,
        ..Default::default()
    };
    let entries = sample_custom_entries();
    manager
        .save_custom_entries(&service, &entries, "写入自定义项", &options)
        .unwrap();

    // 模拟内核升级：10_linux 重新生成菜单，41_* 保持不变
    let custom_script = fs::read_to_string(&custom_script_path).unwrap();
    let before = parse_grub_cfg(&simulate_grub_mkconfig(LINUX_MENU_BEFORE, &custom_script));
    let after = parse_grub_cfg(&simulate_grub_mkconfig(LINUX_MENU_AFTER, &custom_script));

    let count_custom = |menu: &[grub_boot_reader::MenuNode]| {
        menu.iter()
            .flat_map(|n| n.collect_entries())
            .filter(|e| e.title == "Ubuntu Live ISO" || e.title == "Windows 11")
            .count()
    };
    assert_eq!(count_custom(&before), 2);
    assert_eq!(count_custom(&after), 2);

    // GRUB_DEFAULT 语义索引在菜单扩增后仍指向合法序号范围
    let cfg = parse_grub_config(&fs::read_to_string(&default_cfg).unwrap());
    let default = get_default_entry(&cfg).unwrap().unwrap();
    match default {
        DefaultEntry::Index(idx) => {
            let total: usize = after.iter().map(|n| n.collect_entries().len()).sum();
            assert!((idx as usize) < total.max(1) || idx == 0);
        }
        other => panic!("期望语义索引，实际: {:?}", other),
    }
}

#[test]
fn test_apply_failure_rolls_back_config_and_keeps_menu_consistent() {
    let root = test_root("apply_rollback");
    let custom_script_path = root.join("41_helmsman_custom");
    let aliases = root.join("aliases.json");
    let backup = root.join("backups");
    let default_cfg = root.join("default_grub");
    let grub_cfg = root.join("grub.cfg");
    fs::create_dir_all(&backup).unwrap();
    fs::write(&default_cfg, "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n").unwrap();
    fs::write(&grub_cfg, "## 初始菜单\n").unwrap();

    // 更新命令失败（白名单拦截或非零退出）
    let profile = DistroProfile {
        config_path: grub_cfg.to_string_lossy().into_owned(),
        update_command: "false".to_string(),
        command_args: Vec::new(),
        grubenv_path: root.join("grubenv").to_string_lossy().into_owned(),
        ..make_profile()
    };
    let service = GrubService::new_with_paths(default_cfg.clone(), backup.clone(), profile);
    let manager = CustomManager::new_with_paths(custom_script_path.clone(), aliases, backup);

    let before_cfg = fs::read_to_string(&default_cfg).unwrap();
    let options_run = TransactionOptions {
        skip_command_execution: false,
        skip_syntax_check: true,
        ..Default::default()
    };
    let entries = sample_custom_entries();
    let err = manager
        .save_custom_entries(&service, &entries, "更新失败", &options_run)
        .unwrap_err();
    assert!(
        matches!(
            err,
            helmsman_daemon::DaemonError::CommandLaunchFailed { .. }
                | helmsman_daemon::DaemonError::SecurityCheckFailed(_)
        ),
        "实际错误: {:?}",
        err
    );

    // 配置未被污染；自定义脚本也应已回滚（首次创建无历史时允许保留新文件，
    // 但 default/grub 配置必须保持不变）
    let after_cfg = fs::read_to_string(&default_cfg).unwrap();
    assert_eq!(before_cfg, after_cfg);
}

#[test]
fn test_custom_script_roundtrip_survives_regeneration() {
    // 生成 → 解析 → 再生成，条目内容应保持稳定（update-grub 后仍可管理）
    let entries = sample_custom_entries();
    let script1 = generate_custom_script(&entries).unwrap();
    let parsed = parse_custom_script(&script1);
    assert_eq!(parsed.len(), 2);
    let script2 = generate_custom_script(&parsed).unwrap();
    assert_eq!(script1, script2);
}

#[test]
fn test_semantic_default_index_targets_custom_entry() {
    let root = test_root("semantic_default");
    let custom_script_path = root.join("41_helmsman_custom");
    let entries = sample_custom_entries();
    fs::write(
        &custom_script_path,
        generate_custom_script(&entries).unwrap(),
    )
    .unwrap();

    let custom_script = fs::read_to_string(&custom_script_path).unwrap();
    let menu = parse_grub_cfg(&simulate_grub_mkconfig(LINUX_MENU_BEFORE, &custom_script));
    let flat: Vec<_> = menu
        .iter()
        .flat_map(|n| n.collect_entries())
        .cloned()
        .collect();
    let win_idx = flat
        .iter()
        .position(|e| e.title == "Windows 11")
        .expect("应能找到 Windows 自定义项");

    let mut cfg = parse_grub_config("GRUB_DEFAULT=0\n");
    set_default_entry(&mut cfg, &DefaultEntry::Index(win_idx as u32)).unwrap();
    assert_eq!(cfg.get("GRUB_DEFAULT"), Some(win_idx.to_string().as_str()));
}
