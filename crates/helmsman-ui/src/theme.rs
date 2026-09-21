/// 界面视觉主题模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// 现代暗色主题 (默认)
    #[default]
    Dark,
    /// 浅色明亮主题
    Light,
    /// 无障碍高对比度主题 (符合 WCAG AA 级标准)
    HighContrast,
}

/// 字体栈与文字渲染配置
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontConfig {
    /// 首选字体家族列表（按照优先级顺序排列的中文字体回退链）
    pub font_families: Vec<String>,
    /// 基础字体大小 (pt / 逻辑像素)
    pub base_font_size: u16,
    /// 标题字体大小
    pub title_font_size: u16,
    /// 代码与配置预览等宽字体家族
    pub monospace_family: String,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            font_families: vec![
                "Noto Sans CJK SC".to_string(),
                "Source Han Sans SC".to_string(),
                "WenQuanYi Micro Hei".to_string(),
                "sans-serif".to_string(),
            ],
            base_font_size: 14,
            title_font_size: 18,
            monospace_family: "monospace".to_string(),
        }
    }
}

/// 窗口与 Wayland 桌面契约配置
#[derive(Debug, Clone, PartialEq)]
pub struct WindowConfig {
    /// 窗口默认逻辑宽度
    pub default_width: f32,
    /// 窗口默认逻辑高度
    pub default_height: f32,
    /// 窗口最小允许宽度
    pub min_width: f32,
    /// 窗口最小允许高度
    pub min_height: f32,
    /// 窗口标题
    pub title: String,
    /// 是否优先使用 Wayland 原生协议
    pub prefer_wayland: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            default_width: 960.0,
            default_height: 640.0,
            min_width: 720.0,
            min_height: 480.0,
            title: "Helmsman (舵手)".to_string(),
            prefer_wayland: true,
        }
    }
}

impl WindowConfig {
    /// 根据系统分数缩放系数（如 1.25、1.5、2.0）计算物理像素尺寸
    pub fn calculate_physical_size(&self, scale_factor: f64) -> (u32, u32) {
        let valid_scale = if scale_factor > 0.1 {
            scale_factor
        } else {
            1.0
        };
        let w = (self.default_width as f64 * valid_scale).round() as u32;
        let h = (self.default_height as f64 * valid_scale).round() as u32;
        (w, h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_mode_default() {
        let theme = ThemeMode::default();
        assert_eq!(theme, ThemeMode::Dark);
    }

    #[test]
    fn test_font_config_cjk_fallback() {
        let font_cfg = FontConfig::default();
        // 验证第一优先中文字体
        assert_eq!(font_cfg.font_families[0], "Noto Sans CJK SC");
        assert_eq!(font_cfg.font_families[1], "Source Han Sans SC");
        assert_eq!(font_cfg.font_families[2], "WenQuanYi Micro Hei");
        assert_eq!(font_cfg.base_font_size, 14);
    }

    #[test]
    fn test_window_config_hidpi_scaling() {
        let win_cfg = WindowConfig::default();
        assert!(win_cfg.prefer_wayland);

        // 100% 缩放
        let (w1, h1) = win_cfg.calculate_physical_size(1.0);
        assert_eq!(w1, 960);
        assert_eq!(h1, 640);

        // 150% 分数缩放
        let (w15, h15) = win_cfg.calculate_physical_size(1.5);
        assert_eq!(w15, 1440);
        assert_eq!(h15, 960);

        // 200% 高分缩放
        let (w2, h2) = win_cfg.calculate_physical_size(2.0);
        assert_eq!(w2, 1920);
        assert_eq!(h2, 1280);

        // 异常缩放兜底
        let (w_err, h_err) = win_cfg.calculate_physical_size(0.0);
        assert_eq!(w_err, 960);
        assert_eq!(h_err, 640);
    }
}
