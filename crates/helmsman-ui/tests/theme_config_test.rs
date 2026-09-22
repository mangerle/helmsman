use grub_config_parser::parse_grub_config;
use helmsman_ui::i18n::Language;
use helmsman_ui::{AppState, ImpactAnalyzer, RiskLevel, SafetyValidator};

#[test]
fn test_app_state_grub_theme_and_background_crud() {
    let default_grub = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n";
    let grub_cfg = "menuentry 'Ubuntu' { linux /vmlinuz; }\n";
    let mut state = AppState::new_from_content(default_grub, grub_cfg);

    assert!(state.get_grub_theme_path().is_none());
    assert!(state.get_grub_background_path().is_none());
    assert!(state.get_grub_color_normal().is_none());

    // 设置主题路径与背景壁纸
    state.set_grub_theme_path(Some("/boot/grub/themes/vimix/theme.txt"));
    state.set_grub_background_path(Some("/boot/grub/wallpapers/splash.png"));
    state.set_grub_colors(Some("light-gray/black"), Some("black/light-gray"));

    assert_eq!(
        state.get_grub_theme_path(),
        Some("/boot/grub/themes/vimix/theme.txt")
    );
    assert_eq!(
        state.get_grub_background_path(),
        Some("/boot/grub/wallpapers/splash.png")
    );
    assert_eq!(state.get_grub_color_normal(), Some("light-gray/black"));
    assert_eq!(state.get_grub_color_highlight(), Some("black/light-gray"));
    assert!(state.has_unsaved_changes());

    // 一键重置草稿
    state.reset_draft();
    assert!(!state.has_unsaved_changes());
    assert!(state.get_grub_theme_path().is_none());
    assert!(state.get_grub_background_path().is_none());
}

#[test]
fn test_safety_validator_theme_rules() {
    // 1. 合规配置
    let valid_cfg = parse_grub_config(
        "GRUB_THEME=\"/boot/grub/themes/starfield/theme.txt\"\nGRUB_BACKGROUND=\"/boot/bg.png\"\n",
    );
    let issues = SafetyValidator::validate(&valid_cfg, Language::ZhCn);
    assert!(
        !issues
            .iter()
            .any(|i| i.title.contains("主题") || i.title.contains("背景"))
    );

    // 2. 主题路径异常（未以 theme.txt 结尾）
    let invalid_theme_cfg =
        parse_grub_config("GRUB_THEME=\"/boot/grub/themes/starfield/style.conf\"\n");
    let issues_t = SafetyValidator::validate(&invalid_theme_cfg, Language::ZhCn);
    let theme_issue = issues_t
        .iter()
        .find(|i| i.title.contains("主题路径格式异常"))
        .expect("应发现主题路径警告");
    assert_eq!(theme_issue.level, RiskLevel::Warning);

    // 3. 背景壁纸格式异常（如 .bmp）
    let invalid_bg_cfg = parse_grub_config("GRUB_BACKGROUND=\"/boot/image.bmp\"\n");
    let issues_b = SafetyValidator::validate(&invalid_bg_cfg, Language::ZhCn);
    let bg_issue = issues_b
        .iter()
        .find(|i| i.title.contains("背景壁纸格式不支持"))
        .expect("应发现背景壁纸格式警告");
    assert_eq!(bg_issue.level, RiskLevel::Warning);
}

#[test]
fn test_impact_analyzer_theme_and_background() {
    let orig_cfg = parse_grub_config("GRUB_DEFAULT=0\n");

    // 启用主题与背景
    let draft_cfg = parse_grub_config(
        "GRUB_DEFAULT=0\nGRUB_THEME=\"/boot/grub/themes/vimix/theme.txt\"\nGRUB_BACKGROUND=\"/boot/bg.png\"\n",
    );
    let report = ImpactAnalyzer::analyze(&orig_cfg, &draft_cfg, Language::ZhCn);

    let theme_item = report
        .items
        .iter()
        .find(|i| i.title.contains("启用开机视觉主题"))
        .expect("应包含启用主题影响条目");
    assert!(theme_item.explanation.contains("vimix/theme.txt"));

    let bg_item = report
        .items
        .iter()
        .find(|i| i.title.contains("设置自定义开机壁纸"))
        .expect("应包含设置壁纸影响条目");
    assert!(bg_item.explanation.contains("/boot/bg.png"));
}
