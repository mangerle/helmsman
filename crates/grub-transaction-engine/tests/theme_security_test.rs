use grub_transaction_engine::{
    ThemeSecurityError, extract_safe_entries, install_theme_directory, list_installed_themes,
    remove_theme, validate_entry_path, validate_theme_name,
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

#[test]
fn test_install_theme_from_zip_archive_end_to_end() {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let temp_root = std::env::temp_dir().join("helmsman_test_zip_theme");
    let _ = fs::remove_dir_all(&temp_root);
    fs::create_dir_all(&temp_root).unwrap();

    let zip_path = temp_root.join("sample_theme.zip");
    let target_themes_root = temp_root.join("themes_root");

    // 1. 创建测试 ZIP 压缩包
    {
        let file = fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        zip.start_file("theme.txt", options).unwrap();
        zip.write_all(b"title-text: 'Sample Zip Theme'\n").unwrap();

        zip.start_file("background.png", options).unwrap();
        zip.write_all(b"dummy png content").unwrap();

        zip.finish().unwrap();
    }

    // 2. 执行安装
    let installed =
        grub_transaction_engine::install_theme_from_archive(&zip_path, &target_themes_root, None)
            .unwrap();

    assert_eq!(installed, target_themes_root.join("sample_theme"));
    assert!(installed.join("theme.txt").is_file());
    assert!(installed.join("background.png").is_file());

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_install_theme_from_zip_nested_directory() {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let temp_root = std::env::temp_dir().join("helmsman_test_zip_nested");
    let _ = fs::remove_dir_all(&temp_root);
    fs::create_dir_all(&temp_root).unwrap();

    let zip_path = temp_root.join("archive_with_folder.zip");
    let target_themes_root = temp_root.join("themes_root");

    // 创建包含嵌套目录外壳的 ZIP (如 vimix-theme-master/theme.txt)
    {
        let file = fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        zip.add_directory("vimix-dark-master/", options).unwrap();
        zip.start_file("vimix-dark-master/theme.txt", options)
            .unwrap();
        zip.write_all(b"title-text: 'Nested Vimix'\n").unwrap();

        zip.finish().unwrap();
    }

    // 执行安装，自适应识别并安装
    let installed =
        grub_transaction_engine::install_theme_from_archive(&zip_path, &target_themes_root, None)
            .unwrap();

    assert_eq!(installed, target_themes_root.join("vimix-dark-master"));
    assert!(installed.join("theme.txt").is_file());

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_install_theme_from_tar_gz_end_to_end() {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use tar::Builder;

    let temp_root = std::env::temp_dir().join("helmsman_test_targz_theme");
    let _ = fs::remove_dir_all(&temp_root);
    fs::create_dir_all(&temp_root).unwrap();

    let tar_gz_path = temp_root.join("breeze.tar.gz");
    let target_themes_root = temp_root.join("themes_root");

    // 创建 .tar.gz 压缩包
    {
        let file = fs::File::create(&tar_gz_path).unwrap();
        let enc = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(enc);

        let mut header = tar::Header::new_gnu();
        let content = b"title-text: 'Breeze GRUB'\n";
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, "theme.txt", &content[..])
            .unwrap();

        tar.finish().unwrap();
    }

    let installed = grub_transaction_engine::install_theme_from_archive(
        &tar_gz_path,
        &target_themes_root,
        Some("breeze_custom"),
    )
    .unwrap();

    assert_eq!(installed, target_themes_root.join("breeze_custom"));
    assert!(installed.join("theme.txt").is_file());

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_zip_slip_interception_in_archive() {
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    let temp_root = std::env::temp_dir().join("helmsman_test_zip_slip");
    let _ = fs::remove_dir_all(&temp_root);
    fs::create_dir_all(&temp_root).unwrap();

    let zip_path = temp_root.join("evil_slip.zip");
    let target_themes_root = temp_root.join("themes_root");

    // 创建包含恶意相对穿越路径的 ZIP
    {
        let file = fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default();

        zip.start_file("../evil.sh", options).unwrap();
        zip.write_all(b"echo pwned").unwrap();

        zip.finish().unwrap();
    }

    let res =
        grub_transaction_engine::install_theme_from_archive(&zip_path, &target_themes_root, None);

    assert!(matches!(
        res,
        Err(ThemeSecurityError::PathTraversalDetected { .. })
    ));

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_list_and_remove_theme() {
    let temp_root = std::env::temp_dir().join("helmsman_test_list_remove_theme");
    let _ = std::fs::remove_dir_all(&temp_root);
    let themes_root = temp_root.join("themes");
    std::fs::create_dir_all(themes_root.join("vimix")).unwrap();
    std::fs::write(
        themes_root.join("vimix").join("theme.txt"),
        "title-text: V\n",
    )
    .unwrap();
    std::fs::create_dir_all(themes_root.join("tela")).unwrap();
    std::fs::write(
        themes_root.join("tela").join("theme.txt"),
        "title-text: T\n",
    )
    .unwrap();
    std::fs::create_dir_all(themes_root.join("broken")).unwrap();

    let themes = list_installed_themes(&themes_root).unwrap();
    assert_eq!(themes.len(), 3);
    assert_eq!(themes[0].name, "broken");
    assert!(!themes[0].has_descriptor);
    assert_eq!(themes[1].name, "tela");
    assert!(themes[1].has_descriptor);

    remove_theme(&themes_root, "tela").unwrap();
    let after = list_installed_themes(&themes_root).unwrap();
    assert_eq!(after.len(), 2);
    assert!(after.iter().all(|t| t.name != "tela"));

    assert!(remove_theme(&themes_root, "../evil").is_err());
}
