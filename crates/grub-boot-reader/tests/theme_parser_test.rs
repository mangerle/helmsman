use grub_boot_reader::{ThemeDimension, parse_grub_theme, scan_available_themes};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn test_theme_dimension_resolution() {
    let dim_px = ThemeDimension::parse("400px");
    assert_eq!(dim_px, ThemeDimension::Pixels(400));
    assert_eq!(dim_px.resolve_pixels(1920), 400);

    let dim_pct = ThemeDimension::parse("50%");
    assert_eq!(dim_pct, ThemeDimension::Percent(50.0));
    assert_eq!(dim_pct.resolve_pixels(1920), 960);
    assert_eq!(dim_pct.resolve_pixels(1080), 540);

    let dim_raw = ThemeDimension::parse(" 300 ");
    assert_eq!(dim_raw, ThemeDimension::Pixels(300));
}

#[test]
fn test_parse_full_theme_txt() {
    let sample = r##"
# 示例 GRUB 主题描述文件
title-text: "Helmsman OS Bootloader"
desktop-image: "background.png"
desktop-color: "#1e1e2e"

+ boot_menu {
  left = 25%
  top = 30%
  width = 50%
  height = 40%
  item_font = "Unifont Regular 16"
  item_color = "#cdd6f4"
  selected_item_color = "#89b4fa"
  item_height = 32
  icon_width = 24
  icon_height = 24
}

+ progress_bar {
  id = "__timeout__"
  left = 25%
  top = 75%
  width = 50%
  height = 20
  fg_color = "#89b4fa"
  bg_color = "#313244"
  border_color = "#45475a"
}

+ label {
  text = "按 Enter 启动，按 'e' 编辑条目"
  color = "#a6adc8"
  align = "center"
  top = 85%
  left = 0
  width = 100%
}
"##;

    let dummy_path = PathBuf::from("/boot/grub/themes/test_theme/theme.txt");
    let theme = parse_grub_theme(sample, &dummy_path);

    assert_eq!(theme.name, "test_theme");
    assert_eq!(theme.title_text.as_deref(), Some("Helmsman OS Bootloader"));
    assert_eq!(theme.desktop_color.as_deref(), Some("#1e1e2e"));
    assert_eq!(
        theme.desktop_image,
        Some(PathBuf::from("/boot/grub/themes/test_theme/background.png"))
    );

    // 检查 boot_menu
    let menu = theme.boot_menu().expect("应成功提取 boot_menu 组件");
    assert_eq!(menu.left, Some(ThemeDimension::Percent(25.0)));
    assert_eq!(menu.width, Some(ThemeDimension::Percent(50.0)));
    assert_eq!(menu.item_color.as_deref(), Some("#cdd6f4"));
    assert_eq!(menu.selected_item_color.as_deref(), Some("#89b4fa"));
    assert_eq!(menu.item_height, Some(32));

    // 检查 progress_bar
    let pbar = theme.progress_bar().expect("应成功提取 progress_bar 组件");
    assert_eq!(pbar.id.as_deref(), Some("__timeout__"));
    assert_eq!(pbar.fg_color.as_deref(), Some("#89b4fa"));
    assert_eq!(pbar.bg_color.as_deref(), Some("#313244"));
}

#[test]
fn test_scan_available_themes_sandbox() {
    let sandbox = std::env::temp_dir().join("helmsman_test_scan_themes");
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).unwrap();

    let theme1_dir = sandbox.join("vimix");
    fs::create_dir_all(&theme1_dir).unwrap();
    fs::write(theme1_dir.join("theme.txt"), "title-text: \"Vimix\"\n").unwrap();
    fs::write(theme1_dir.join("background.png"), "dummy").unwrap();

    let theme2_dir = sandbox.join("tela");
    fs::create_dir_all(&theme2_dir).unwrap();
    fs::write(theme2_dir.join("theme.txt"), "title-text: \"Tela\"\n").unwrap();

    // 非主题目录（无 theme.txt）
    let non_theme_dir = sandbox.join("not_a_theme");
    fs::create_dir_all(&non_theme_dir).unwrap();

    let scanned = scan_available_themes(&[Path::new(&sandbox)]);
    assert_eq!(scanned.len(), 2);

    let vimix = scanned.iter().find(|t| t.name == "vimix").unwrap();
    assert!(vimix.has_desktop_image);
    assert_eq!(
        vimix.preview_image_path,
        Some(theme1_dir.join("background.png"))
    );

    let tela = scanned.iter().find(|t| t.name == "tela").unwrap();
    assert!(!tela.has_desktop_image);

    let _ = fs::remove_dir_all(&sandbox);
}
