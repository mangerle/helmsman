use grub_boot_reader::{
    CustomBootEntry, CustomEntryError, entry_types, generate_custom_script, parse_custom_script,
    validate_custom_script_structure, validate_entry_list,
};

#[test]
fn test_custom_entry_iso_boot_roundtrip() {
    let entry = CustomBootEntry::new_iso_boot(
        "ubuntu_iso",
        "Ubuntu 24.04 Live",
        "/boot/iso/ubuntu.iso",
        "1234-5678",
        "nomodeset",
    );

    let script = generate_custom_script(&[entry]).expect("合法条目应成功生成脚本");
    assert!(script.contains("Ubuntu 24.04 Live"));
    assert!(script.contains("ubuntu.iso"));
    assert!(script.contains("1234-5678"));
    assert!(script.contains("nomodeset"));

    let parsed = parse_custom_script(&script);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].id, "ubuntu_iso");
    assert_eq!(parsed[0].title, "Ubuntu 24.04 Live");
    assert_eq!(parsed[0].entry_type, entry_types::ISO);
    assert_eq!(parsed[0].target_path, "/boot/iso/ubuntu.iso");
    assert_eq!(parsed[0].partition_uuid, "1234-5678");
    assert_eq!(parsed[0].cmdline_params, "nomodeset");
    assert!(parsed[0].enabled);
}

#[test]
fn test_custom_entry_chainloader_and_disabled() {
    let mut entry = CustomBootEntry::new_chainloader(
        "win11",
        "Windows 11",
        "/EFI/Microsoft/Boot/bootmgfw.efi",
        "ABCD-EF01",
    );
    entry.enabled = false;

    let script = generate_custom_script(&[entry]).expect("合法条目应成功生成脚本");
    assert!(script.contains("# menuentry 'Windows 11'"));

    let parsed = parse_custom_script(&script);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].id, "win11");
    assert_eq!(parsed[0].title, "Windows 11");
    assert_eq!(parsed[0].entry_type, entry_types::CHAINLOADER);
    assert_eq!(parsed[0].target_path, "/EFI/Microsoft/Boot/bootmgfw.efi");
    assert_eq!(parsed[0].partition_uuid, "ABCD-EF01");
    assert!(!parsed[0].enabled);
}

#[test]
fn test_reject_title_quote_injection() {
    let entry = CustomBootEntry::new_iso_boot(
        "evil",
        "x' } { menuentry pwned",
        "/boot/iso/a.iso",
        "ABCD",
        "",
    );
    assert!(matches!(
        entry.validate(),
        Err(CustomEntryError::InvalidTitle { .. })
    ));
    assert!(generate_custom_script(&[entry]).is_err());
}

#[test]
fn test_reject_id_injection() {
    let entry = CustomBootEntry::new_raw_script("bad\" id=\"x", "标题", "true");
    assert!(matches!(
        entry.validate(),
        Err(CustomEntryError::InvalidId { .. })
    ));
}

#[test]
fn test_reject_path_metacharacters() {
    let entry =
        CustomBootEntry::new_iso_boot("iso1", "标题", "/boot/iso/a.iso; rm -rf /", "ABCD", "");
    assert!(matches!(
        entry.validate(),
        Err(CustomEntryError::InvalidPath { .. })
    ));
}

#[test]
fn test_reject_raw_script_brace_escape() {
    let entry =
        CustomBootEntry::new_raw_script("raw1", "标题", "true\n}\nmenuentry 'pwned' {\ntrue");
    assert!(matches!(
        entry.validate(),
        Err(CustomEntryError::InvalidRawScript { .. })
    ));
}

#[test]
fn test_reject_cmdline_injection() {
    let entry = CustomBootEntry::new_custom_linux(
        "lin1",
        "标题",
        "/vmlinuz",
        "/initrd.img",
        "ABCD-EF",
        "quiet $(reboot)",
    );
    assert!(matches!(
        entry.validate(),
        Err(CustomEntryError::InvalidCmdline { .. })
    ));
}

#[test]
fn test_validate_entry_list_conflicts() {
    let a = CustomBootEntry::new_iso_boot("id_a", "标题 A", "/iso/a.iso", "uuid-a", "");
    let mut dup_id = a.clone();
    dup_id.title = "标题 B".to_string();
    assert!(matches!(
        validate_entry_list(&[a.clone(), dup_id]),
        Err(CustomEntryError::Conflict { .. })
    ));

    let mut dup_title = a.clone();
    dup_title.id = "id_b".to_string();
    assert!(matches!(
        validate_entry_list(&[a, dup_title]),
        Err(CustomEntryError::Conflict { .. })
    ));
}

#[test]
fn test_generate_script_structure_validation() {
    let entry = CustomBootEntry::new_iso_boot("iso_1", "Live", "/iso/a.iso", "uuid-a", "");
    let mut disabled = entry.clone();
    disabled.id = "iso_2".to_string();
    disabled.title = "Live 禁用".to_string();
    disabled.enabled = false;

    let script = generate_custom_script(&[entry, disabled]).unwrap();
    assert!(validate_custom_script_structure(&script).is_ok());
    assert_eq!(
        script
            .lines()
            .filter(|l| l.trim_start().starts_with("menuentry "))
            .count(),
        1
    );

    let broken = script.replace("{\n", "\n");
    assert!(matches!(
        validate_custom_script_structure(&broken),
        Err(CustomEntryError::InvalidScriptStructure { .. })
    ));
}

#[test]
fn test_submenu_grouping_roundtrip() {
    let root = CustomBootEntry::new_iso_boot("iso_root", "根条目", "/iso/r.iso", "uuid-r", "");
    let in_tools = CustomBootEntry::new_iso_boot("iso_a", "工具 A", "/iso/a.iso", "uuid-a", "")
        .with_submenu("工具箱");
    let in_tools_b = CustomBootEntry::new_iso_boot("iso_b", "工具 B", "/iso/b.iso", "uuid-b", "")
        .with_submenu("工具箱");

    let script = generate_custom_script(&[root.clone(), in_tools.clone(), in_tools_b.clone()])
        .expect("子菜单分组应成功生成");
    assert!(script.contains("submenu '工具箱' {"));
    assert!(validate_custom_script_structure(&script).is_ok());

    let parsed = parse_custom_script(&script);
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0].id, "iso_root");
    assert_eq!(parsed[0].submenu_title, "");
    let a = parsed.iter().find(|e| e.id == "iso_a").unwrap();
    assert_eq!(a.submenu_title, "工具箱");
    let b = parsed.iter().find(|e| e.id == "iso_b").unwrap();
    assert_eq!(b.submenu_title, "工具箱");

    let mut moved = a.clone();
    moved.submenu_title = String::new();
    let script2 = generate_custom_script(&[root, moved, in_tools_b]).unwrap();
    assert!(
        !script2.contains("submenu '工具箱' {")
            || script2
                .lines()
                .filter(|l| l.trim_start().starts_with("menuentry "))
                .count()
                >= 2
    );
    assert!(script2.contains("menuentry '工具 A'"));
}
