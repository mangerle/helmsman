use grub_transaction_engine::{
    ThemeSecurityError, extract_safe_entries, install_theme_directory, validate_entry_path,
    validate_theme_name,
};
use std::fs;
use std::path::PathBuf;

#[test]
fn test_validate_theme_name() {
    assert!(validate_theme_name("vimix").is_ok());
    assert!(validate_theme_name("breeze-dark-2026").is_ok());

    // 拒绝非法名称
    assert!(validate_theme_name("").is_err());
    assert!(validate_theme_name("../evil").is_err());
    assert!(validate_theme_name("sub/dir").is_err());
    assert!(validate_theme_name("sub\\dir").is_err());
}

#[test]
fn test_path_traversal_detection() {
    let sandbox = PathBuf::from("/boot/grub/themes/test_theme");

    // 1. 普通相对父目录遍历
    let err1 = validate_entry_path(&sandbox, "../evil.sh");
    assert!(matches!(
        err1,
        Err(ThemeSecurityError::PathTraversalDetected { .. })
    ));

    // 2. 多级嵌套逃逸
    let err2 = validate_entry_path(&sandbox, "icons/../../../../etc/shadow");
    assert!(matches!(
        err2,
        Err(ThemeSecurityError::PathTraversalDetected { .. })
    ));

    // 3. 根目录绝对路径
    let err3 = validate_entry_path(&sandbox, "/etc/passwd");
    assert!(matches!(
        err3,
        Err(ThemeSecurityError::PathTraversalDetected { .. })
    ));

    // 4. Windows 风格反斜杠穿越
    let err4 = validate_entry_path(&sandbox, "..\\..\\windows\\system32");
    assert!(matches!(
        err4,
        Err(ThemeSecurityError::PathTraversalDetected { .. })
    ));

    // 5. 合法子目录与文件
    let ok1 = validate_entry_path(&sandbox, "theme.txt").unwrap();
    assert_eq!(ok1, sandbox.join("theme.txt"));

    let ok2 = validate_entry_path(&sandbox, "icons/ubuntu.png").unwrap();
    assert_eq!(ok2, sandbox.join("icons/ubuntu.png"));
}

#[test]
fn test_extract_safe_entries_lifecycle() {
    let sandbox = std::env::temp_dir().join("helmsman_test_extract_theme");
    let _ = fs::remove_dir_all(&sandbox);

    // 1. 尝试解压缺少 theme.txt 的残缺包：应被拦截
    let incomplete_entries = vec![("background.png", false, &b"image data"[..])];
    let res_incomplete = extract_safe_entries(&sandbox, incomplete_entries);
    assert!(matches!(
        res_incomplete,
        Err(ThemeSecurityError::MissingThemeDescriptor { .. })
    ));

    // 2. 尝试解压包含恶意穿越路径的包：应被拦截
    let malicious_entries = vec![
        ("theme.txt", false, &b"title-text: test"[..]),
        ("../evil.sh", false, &b"echo hack"[..]),
    ];
    let res_malicious = extract_safe_entries(&sandbox, malicious_entries);
    assert!(matches!(
        res_malicious,
        Err(ThemeSecurityError::PathTraversalDetected { .. })
    ));

    // 3. 正常完整主题包解压
    let valid_entries = vec![
        ("theme.txt", false, &b"title-text: test"[..]),
        ("background.png", false, &b"image"[..]),
        ("icons/arch.png", false, &b"icon"[..]),
    ];
    let res_valid = extract_safe_entries(&sandbox, valid_entries);
    assert!(res_valid.is_ok());

    assert!(sandbox.join("theme.txt").is_file());
    assert!(sandbox.join("background.png").is_file());
    assert!(sandbox.join("icons/arch.png").is_file());

    let _ = fs::remove_dir_all(&sandbox);
}

#[test]
fn test_install_theme_directory_end_to_end() {
    let temp_root = std::env::temp_dir().join("helmsman_test_install_theme");
    let _ = fs::remove_dir_all(&temp_root);

    let source_dir = temp_root.join("source_theme");
    let target_root = temp_root.join("system_themes");

    fs::create_dir_all(&source_dir).unwrap();
    fs::write(source_dir.join("theme.txt"), "title-text: Installed\n").unwrap();
    fs::write(source_dir.join("background.png"), "png").unwrap();

    let res = install_theme_directory(&source_dir, &target_root, "installed_theme");
    assert!(res.is_ok());

    let target_theme_dir = res.unwrap();
    assert_eq!(target_theme_dir, target_root.join("installed_theme"));
    assert!(target_theme_dir.join("theme.txt").is_file());
    assert!(target_theme_dir.join("background.png").is_file());

    let _ = fs::remove_dir_all(&temp_root);
}
