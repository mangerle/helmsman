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

    // 3. 用户追加内核参数
    state.set_cmdline_default("quiet splash nomodeset");
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
