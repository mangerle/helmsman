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
    state.set_timeout(5);
    assert!(state.has_unsaved_changes());
    assert_eq!(state.get_timeout(), 5);

    // 2. 用户选择 Windows 为默认启动项
    state.set_default_entry("Windows Boot Manager (on /dev/nvme0n1p1)");
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
