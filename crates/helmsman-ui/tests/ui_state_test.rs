use helmsman_ui::AppState;

const SAMPLE_DEFAULT_GRUB: &str = r#"GRUB_DEFAULT=0
GRUB_TIMEOUT=0
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash"
"#;

const SAMPLE_GRUB_CFG: &str = r#"
menuentry 'Ubuntu' --class ubuntu --class os $menuentry_id_option 'gnulinux-simple-5452f864' {
	linux	/boot/vmlinuz-6.8.0-40-generic root=UUID=5452f864 ro quiet splash
	initrd	/boot/initrd.img-6.8.0-40-generic
}
submenu 'Advanced options for Ubuntu' {
	menuentry 'Ubuntu, with Linux 6.8.0-40-generic' --class ubuntu {
		linux	/boot/vmlinuz-6.8.0-40-generic root=UUID=5452f864 ro quiet splash
		initrd	/boot/initrd.img-6.8.0-40-generic
	}
}
menuentry 'Windows Boot Manager (on /dev/nvme0n1p1)' --class windows {
	chainloader /EFI/Microsoft/Boot/bootmgfw.efi
}
"#;

#[test]
fn test_ui_state_lifecycle() {
    let mut state = AppState::new_from_content(SAMPLE_DEFAULT_GRUB, SAMPLE_GRUB_CFG);

    // 初始状态应无未保存修改
    assert!(!state.has_unsaved_changes());

    // 验证默认选中的条目
    let selected = state.get_selected_entry().unwrap();
    assert_eq!(selected.title, "Ubuntu");
    assert_eq!(
        selected.kernel_path.as_deref(),
        Some("/boot/vmlinuz-6.8.0-40-generic")
    );

    // 1. 用户修改倒计时为 5
    state.set_timeout(5).expect("倒计时 5 秒应合法");
    assert!(state.has_unsaved_changes());
    assert_eq!(state.get_timeout(), 5);

    // 2. 用户选择 Windows 为默认启动项
    state
        .set_default_entry("Windows Boot Manager (on /dev/nvme0n1p1)")
        .expect("Windows 标题应合法");
    assert_eq!(
        state.get_default_entry(),
        Some("Windows Boot Manager (on /dev/nvme0n1p1)")
    );

    // 3. 用户追加内核参数（自动去重）
    state
        .set_cmdline_default("quiet splash nomodeset quiet")
        .expect("参数串应合法");
    assert_eq!(state.get_cmdline_default(), "quiet splash nomodeset");

    // 4. 验证 Diff 计算
    let diff = state.compute_diff();
    assert!(diff.has_changes);
    assert!(diff.diff_text.contains("-GRUB_TIMEOUT=0"));
    assert!(diff.diff_text.contains("+GRUB_TIMEOUT=5"));
    assert!(diff.diff_text.contains("-GRUB_DEFAULT=0"));
    assert!(
        diff.diff_text
            .contains("+GRUB_DEFAULT=\"Windows Boot Manager (on /dev/nvme0n1p1)\"")
    );

    // 5. 选中 Windows 条目，检查器数据更新
    state.select_entry("Windows Boot Manager (on /dev/nvme0n1p1)");
    let win_entry = state.get_selected_entry().unwrap();
    assert_eq!(win_entry.title, "Windows Boot Manager (on /dev/nvme0n1p1)");
    assert_eq!(
        win_entry.chainloader_path.as_deref(),
        Some("/EFI/Microsoft/Boot/bootmgfw.efi")
    );
}

#[test]
fn test_saved_default_mode() {
    let mut state = AppState::new_from_content(SAMPLE_DEFAULT_GRUB, SAMPLE_GRUB_CFG);

    // 初始状态下未启用 saved 模式
    assert!(!state.is_saved_default_enabled());

    // 启用 saved 模式
    state.enable_saved_default_mode();
    assert!(state.is_saved_default_enabled());
    assert_eq!(state.get_default_entry(), Some("saved"));
    assert_eq!(state.draft_config.get("GRUB_SAVEDEFAULT"), Some("true"));
}

#[test]
fn test_timeout_style_typed_contract() {
    use helmsman_ui::TimeoutStyle;

    let mut state = AppState::new_from_content(SAMPLE_DEFAULT_GRUB, SAMPLE_GRUB_CFG);

    // 缺省展示风格为 menu
    assert_eq!(state.get_timeout_style(), TimeoutStyle::Menu);
    assert_eq!(state.get_timeout_style_raw(), "menu");

    state
        .set_timeout_style(TimeoutStyle::Hidden)
        .expect("hidden 风格应合法");
    assert_eq!(state.get_timeout_style(), TimeoutStyle::Hidden);
    assert_eq!(state.draft_config.get("GRUB_TIMEOUT_STYLE"), Some("hidden"));
}

#[test]
fn test_timeout_and_default_semantic_index() {
    use helmsman_ui::DefaultEntry;

    let mut state = AppState::new_from_content(SAMPLE_DEFAULT_GRUB, SAMPLE_GRUB_CFG);

    // 无限等待合法
    state.set_timeout(-1).expect("-1 应合法");
    assert_eq!(state.get_timeout(), -1);
    // 越界拒绝且不写入
    assert!(state.set_timeout(-2).is_err());
    assert_eq!(state.get_timeout(), -1);

    // 解析 Windows 条目语义索引后写入 GRUB_DEFAULT
    let index = state
        .resolve_menu_index("Windows Boot Manager (on /dev/nvme0n1p1)")
        .expect("应能解析 Windows 条目序号");
    assert_eq!(index, 2);
    state
        .set_default_entry_semantic("Windows Boot Manager (on /dev/nvme0n1p1)")
        .expect("语义索引写入应成功");
    assert_eq!(state.get_default_entry(), Some("2"));
    assert_eq!(
        state.get_default_entry_typed().unwrap(),
        Some(DefaultEntry::Index(2))
    );

    // 格式保真：原有 GRUB_TIMEOUT 键序与注释不被打乱
    let text = state.draft_config.serialize();
    assert!(text.contains("GRUB_DEFAULT=2"));
    assert!(text.contains("GRUB_TIMEOUT=-1"));
}

#[test]
fn test_default_entry_invalid_rejected() {
    let mut state = AppState::new_from_content(SAMPLE_DEFAULT_GRUB, SAMPLE_GRUB_CFG);
    assert!(state.set_default_entry("   ").is_err());
    assert!(state.set_default_entry("bad\nline").is_err());
    // 非法值不得落入草稿
    assert_eq!(state.get_default_entry(), Some("0"));
}

#[test]
fn test_kernel_cmdline_dedup_and_flags() {
    use grub_config_parser::well_known_flags;

    let mut state = AppState::new_from_content(SAMPLE_DEFAULT_GRUB, SAMPLE_GRUB_CFG);
    assert_eq!(state.get_cmdline_default(), "quiet splash");

    // 重复 quiet 应被去重
    state
        .set_cmdline_default("quiet splash quiet nomodeset")
        .unwrap();
    assert_eq!(state.get_cmdline_default(), "quiet splash nomodeset");

    // 开关启停
    state
        .set_cmdline_default_flag(well_known_flags::SPLASH, false)
        .unwrap();
    assert_eq!(state.get_cmdline_default(), "quiet nomodeset");
    state
        .set_cmdline_default_flag(well_known_flags::SPLASH, true)
        .unwrap();
    assert_eq!(state.get_cmdline_default(), "quiet nomodeset splash");

    // 危险令牌拒绝且不污染草稿
    assert!(state.set_cmdline_default("quiet; evil").is_err());
    assert_eq!(state.get_cmdline_default(), "quiet nomodeset splash");
}

#[test]
fn test_class_level_menu_visibility() {
    use helmsman_ui::MenuVisibility;

    let mut state = AppState::new_from_content(SAMPLE_DEFAULT_GRUB, SAMPLE_GRUB_CFG);

    // 默认全部显示
    assert!(!state.get_menu_visibility().has_any_hidden());
    assert!(!state.is_recovery_disabled());

    // 类级隐藏恢复模式
    state.set_recovery_disabled(true);
    assert!(state.is_recovery_disabled());
    assert!(state.get_menu_visibility().recovery_disabled);

    // 拉平子菜单 + 禁用 os-prober
    state.set_submenu_disabled(true);
    state.set_os_prober_enabled(false);
    let vis = state.get_menu_visibility();
    assert!(vis.submenu_disabled);
    assert!(vis.os_prober_disabled);

    // 整体策略写入/恢复
    state.set_menu_visibility(MenuVisibility::default());
    assert!(!state.get_menu_visibility().has_any_hidden());
    assert!(!state.is_submenu_disabled());
}

#[test]
fn test_delete_only_custom_entries() {
    use grub_boot_reader::CustomBootEntry;

    let mut state = AppState::new_from_content(SAMPLE_DEFAULT_GRUB, SAMPLE_GRUB_CFG);

    // 系统条目不可删除
    assert!(state.is_system_entry("Ubuntu"));
    assert!(state.is_system_entry("Windows Boot Manager (on /dev/nvme0n1p1)"));

    // 自定义项可删除
    let custom = CustomBootEntry::new_iso_boot("iso_x", "自定义项", "/iso/x.iso", "uuid-x", "");
    state.add_custom_entry(custom);
    assert!(!state.is_system_entry("iso_x"));
    assert!(state.remove_custom_entry("iso_x"));
    assert!(!state.remove_custom_entry("iso_x"));
}
