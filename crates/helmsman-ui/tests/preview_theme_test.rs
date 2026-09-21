use grub_boot_reader::parse_grub_theme;
use helmsman_ui::{AppState, MenuPreviewModel, Rect};
use std::path::PathBuf;

#[test]
fn test_default_text_preview_layout() {
    let default_grub = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n";
    let grub_cfg = "menuentry 'Ubuntu' {}\n";
    let state = AppState::new_from_content(default_grub, grub_cfg);

    let preview = MenuPreviewModel::from_app_state(&state);
    let layout = preview.compute_visual_layout(1920, 1080);

    assert_eq!(layout.screen_width, 1920);
    assert_eq!(layout.screen_height, 1080);
    assert_eq!(layout.background_color, "#000000");
    assert!(layout.background_image.is_none());
    assert_eq!(layout.title_text.as_deref(), Some("GNU GRUB"));

    // 默认居中矩形
    assert_eq!(
        layout.menu_rect,
        Rect {
            x: 384,
            y: 216,
            width: 1152,
            height: 648,
        }
    );
    assert!(layout.progress_bar_rect.is_none());
    assert_eq!(layout.progress_ratio, 1.0);
}

#[test]
fn test_themed_preview_layout_resolution() {
    let default_grub = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=10\n";
    let grub_cfg = "menuentry 'Ubuntu' {}\nmenuentry 'Windows' {}\n";
    let state = AppState::new_from_content(default_grub, grub_cfg);

    let theme_content = r##"
title-text: "Custom Theme"
desktop-image: "background.png"
desktop-color: "#1e1e2e"

+ boot_menu {
  left = 20%
  top = 25%
  width = 60%
  height = 50%
  item_color = "#cdd6f4"
  selected_item_color = "#89b4fa"
}

+ progress_bar {
  id = "__timeout__"
  left = 20%
  top = 80%
  width = 60%
  height = 24
  fg_color = "#89b4fa"
  bg_color = "#313244"
}
"##;

    let theme_path = PathBuf::from("/boot/grub/themes/test_theme/theme.txt");
    let theme_def = parse_grub_theme(theme_content, &theme_path);

    let mut preview = MenuPreviewModel::from_app_state(&state).with_theme_definition(theme_def);

    // 1. 验证初始 1920x1080 分辨率下的布局
    let layout1 = preview.compute_visual_layout(1920, 1080);
    assert_eq!(layout1.background_color, "#1e1e2e");
    assert_eq!(
        layout1.background_image,
        Some(PathBuf::from("/boot/grub/themes/test_theme/background.png"))
    );
    assert_eq!(layout1.title_text.as_deref(), Some("Custom Theme"));

    assert_eq!(
        layout1.menu_rect,
        Rect {
            x: 384, // 1920 * 0.20
            y: 270, // 1080 * 0.25
            width: 1152,
            height: 540,
        }
    );
    assert_eq!(layout1.item_color, "#cdd6f4");
    assert_eq!(layout1.selected_item_color, "#89b4fa");

    assert_eq!(
        layout1.progress_bar_rect,
        Some(Rect {
            x: 384,
            y: 864, // 1080 * 0.80
            width: 1152,
            height: 24,
        })
    );
    assert_eq!(layout1.progress_bar_fg_color.as_deref(), Some("#89b4fa"));
    assert_eq!(layout1.progress_bar_bg_color.as_deref(), Some("#313244"));
    assert_eq!(layout1.progress_ratio, 1.0);

    // 2. 模拟时间推进 5 秒 (剩余 5/10)
    for _ in 0..5 {
        preview.tick_second();
    }
    let layout2 = preview.compute_visual_layout(1920, 1080);
    assert_eq!(layout2.progress_ratio, 0.5);
}
