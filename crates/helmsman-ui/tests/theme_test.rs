use helmsman_ui::{
    AppState, ColorPalette, FontConfig, FontScale, ResolvedTheme, RgbaColor, SystemColorScheme,
    ThemeMode,
};

#[test]
fn test_theme_resolution_matrix() {
    // 1. 跟随系统 (System) 模式
    assert_eq!(
        ThemeMode::System.resolve(SystemColorScheme::PreferDark),
        ResolvedTheme::Dark
    );
    assert_eq!(
        ThemeMode::System.resolve(SystemColorScheme::PreferLight),
        ResolvedTheme::Light
    );
    assert_eq!(
        ThemeMode::System.resolve(SystemColorScheme::NoPreference),
        ResolvedTheme::Dark
    );

    // 2. 强制深色 (Dark) 模式
    assert_eq!(
        ThemeMode::Dark.resolve(SystemColorScheme::PreferLight),
        ResolvedTheme::Dark
    );

    // 3. 强制浅色 (Light) 模式
    assert_eq!(
        ThemeMode::Light.resolve(SystemColorScheme::PreferDark),
        ResolvedTheme::Light
    );

    // 4. 无障碍高对比度 (HighContrast) 模式
    assert_eq!(
        ThemeMode::HighContrast.resolve(SystemColorScheme::PreferDark),
        ResolvedTheme::HighContrast
    );
    assert_eq!(
        ThemeMode::HighContrast.resolve(SystemColorScheme::PreferLight),
        ResolvedTheme::HighContrast
    );
}

#[test]
fn test_palette_semantic_consistency() {
    let dark_palette = ColorPalette::dark();
    let light_palette = ColorPalette::light();
    let hc_palette = ColorPalette::high_contrast();

    // 验证深色模式：背景深于文字
    assert!(dark_palette.background.r < dark_palette.text_primary.r);

    // 验证浅色模式：背景浅于文字
    assert!(light_palette.background.r > light_palette.text_primary.r);

    // 验证高对比度模式：背景纯黑、文字纯白、边框纯白，实现最高辨识度
    assert_eq!(hc_palette.background, RgbaColor::from_rgb(0, 0, 0));
    assert_eq!(hc_palette.text_primary, RgbaColor::from_rgb(255, 255, 255));
    assert_eq!(hc_palette.border, RgbaColor::from_rgb(255, 255, 255));

    // 验证差异色块透明度设置合理（非零且小于 1.0）
    assert!(dark_palette.diff_add_bg.a > 0.0 && dark_palette.diff_add_bg.a < 1.0);
    assert!(light_palette.diff_remove_bg.a > 0.0 && light_palette.diff_remove_bg.a < 1.0);
}

#[test]
fn test_font_scale_integration() {
    let font_cfg = FontConfig::default();

    // 标准 100%
    assert_eq!(font_cfg.scaled_base_size(FontScale::Standard), 14);
    assert_eq!(font_cfg.scaled_title_size(FontScale::Standard), 18);

    // 125% 放大
    assert_eq!(font_cfg.scaled_base_size(FontScale::Medium), 18);

    // 150% 放大
    assert_eq!(font_cfg.scaled_base_size(FontScale::Large), 21);

    // 200% 视障友好超大字号
    assert_eq!(font_cfg.scaled_base_size(FontScale::ExtraLarge), 28);
    assert_eq!(font_cfg.scaled_title_size(FontScale::ExtraLarge), 36);
}

#[test]
fn test_app_state_theme_switching() {
    let default_grub = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n";
    let grub_cfg = "menuentry 'Ubuntu' {\n  linux /vmlinuz\n}\n";
    let mut state = AppState::new_from_content(default_grub, grub_cfg);

    // 默认主题为 System，默认缩放为 Standard
    assert_eq!(state.get_theme_mode(), ThemeMode::System);
    assert_eq!(state.get_font_scale(), FontScale::Standard);

    // 在系统浅色偏好下，获取到的调色板应为浅色
    let palette_light = state.current_palette(SystemColorScheme::PreferLight);
    assert_eq!(palette_light.background, ColorPalette::light().background);

    // 切换至高对比度模式
    state.set_theme_mode(ThemeMode::HighContrast);
    assert_eq!(state.get_theme_mode(), ThemeMode::HighContrast);

    let palette_hc = state.current_palette(SystemColorScheme::PreferLight);
    assert_eq!(
        palette_hc.background,
        ColorPalette::high_contrast().background
    );

    // 切换字体缩放至大字号
    state.set_font_scale(FontScale::Large);
    assert_eq!(state.get_font_scale(), FontScale::Large);
}
