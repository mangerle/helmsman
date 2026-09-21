use std::fmt;

/// 系统桌面外观配色偏好（遵循 XDG Desktop Portal 规范）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SystemColorScheme {
    /// 未指定偏好（或系统环境未声明）
    #[default]
    NoPreference,
    /// 偏好深色模式 (值: 1)
    PreferDark,
    /// 偏好浅色模式 (值: 2)
    PreferLight,
}

/// 实际生效的视觉主题
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedTheme {
    /// 深色暗黑主题
    Dark,
    /// 浅色明亮主题
    Light,
    /// 无障碍高对比度主题
    HighContrast,
}

/// 界面视觉主题模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeMode {
    /// 跟随系统桌面偏好 (默认)
    #[default]
    System,
    /// 现代暗色主题
    Dark,
    /// 浅色明亮主题
    Light,
    /// 无障碍高对比度主题 (符合 WCAG AA/AAA 级标准)
    HighContrast,
}

impl ThemeMode {
    /// 结合系统桌面偏好解析出最终生效的具体主题
    pub fn resolve(&self, system_preference: SystemColorScheme) -> ResolvedTheme {
        match self {
            ThemeMode::System => match system_preference {
                SystemColorScheme::PreferLight => ResolvedTheme::Light,
                SystemColorScheme::PreferDark | SystemColorScheme::NoPreference => {
                    ResolvedTheme::Dark
                }
            },
            ThemeMode::Dark => ResolvedTheme::Dark,
            ThemeMode::Light => ResolvedTheme::Light,
            ThemeMode::HighContrast => ResolvedTheme::HighContrast,
        }
    }
}

/// RGBA 颜色表示
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

impl RgbaColor {
    /// 创建不透明 RGB 颜色
    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// 创建带透明度的 RGBA 颜色
    pub const fn from_rgba(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// 从十六进制字符串解析颜色 (如 "#1e1e2e", "#fff", "#1e1e2eff")
    pub fn from_hex(hex: &str) -> Result<Self, String> {
        let clean = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
        match clean.len() {
            3 => {
                let r =
                    u8::from_str_radix(&clean[0..1].repeat(2), 16).map_err(|e| e.to_string())?;
                let g =
                    u8::from_str_radix(&clean[1..2].repeat(2), 16).map_err(|e| e.to_string())?;
                let b =
                    u8::from_str_radix(&clean[2..3].repeat(2), 16).map_err(|e| e.to_string())?;
                Ok(Self::from_rgb(r, g, b))
            }
            6 => {
                let r = u8::from_str_radix(&clean[0..2], 16).map_err(|e| e.to_string())?;
                let g = u8::from_str_radix(&clean[2..4], 16).map_err(|e| e.to_string())?;
                let b = u8::from_str_radix(&clean[4..6], 16).map_err(|e| e.to_string())?;
                Ok(Self::from_rgb(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&clean[0..2], 16).map_err(|e| e.to_string())?;
                let g = u8::from_str_radix(&clean[2..4], 16).map_err(|e| e.to_string())?;
                let b = u8::from_str_radix(&clean[4..6], 16).map_err(|e| e.to_string())?;
                let a =
                    u8::from_str_radix(&clean[6..8], 16).map_err(|e| e.to_string())? as f32 / 255.0;
                Ok(Self::from_rgba(r, g, b, a))
            }
            _ => Err(format!("不支持的十六进制颜色格式: {}", hex)),
        }
    }

    /// 转换为 6 位十六进制字符串
    pub fn to_hex_string(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl fmt::Display for RgbaColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex_string())
    }
}

/// 语义化调色板
///
/// # 设计原理
/// - **实现初衷**：杜绝在 UI 渲染组件中硬编码绝对色彩数值，通过语义化槽位统一管理主色、背景、文字与告警状态。
/// - **核心优势**：一处定义全局生效，无缝适配暗色、浅色与无障碍高对比度三种不同显示形态。
#[derive(Debug, Clone, PartialEq)]
pub struct ColorPalette {
    /// 窗口主背景色
    pub background: RgbaColor,
    /// 容器/面板背景色
    pub surface: RgbaColor,
    /// 悬浮/卡片高亮背景色
    pub surface_elevated: RgbaColor,
    /// 主要标题与正文文本色
    pub text_primary: RgbaColor,
    /// 次要说明与辅助文本色
    pub text_secondary: RgbaColor,
    /// 边框与分割线颜色
    pub border: RgbaColor,
    /// 主交互强调色（按钮、焦点、单选态）
    pub accent: RgbaColor,
    /// 危险/拦截警示色（高危操作提示）
    pub danger: RgbaColor,
    /// 警告提示色
    pub warning: RgbaColor,
    /// 成功指示色
    pub success: RgbaColor,
    /// 差异可视化：新增行高亮背景色
    pub diff_add_bg: RgbaColor,
    /// 差异可视化：删除行高亮背景色
    pub diff_remove_bg: RgbaColor,
}

impl ColorPalette {
    /// 现代暗色调色板
    pub fn dark() -> Self {
        Self {
            background: RgbaColor::from_rgb(30, 30, 46), // 深夜底色
            surface: RgbaColor::from_rgb(40, 40, 60),    // 模块面板色
            surface_elevated: RgbaColor::from_rgb(52, 52, 78), // 卡片悬浮色
            text_primary: RgbaColor::from_rgb(205, 214, 244), // 高亮正文色
            text_secondary: RgbaColor::from_rgb(166, 173, 200), // 次级说明色
            border: RgbaColor::from_rgb(69, 71, 90),     // 柔和分割线
            accent: RgbaColor::from_rgb(137, 180, 250),  // 活力蓝强调色
            danger: RgbaColor::from_rgb(243, 139, 168),  // 柔和红危险色
            warning: RgbaColor::from_rgb(249, 226, 175), // 柔和黄警示色
            success: RgbaColor::from_rgb(166, 227, 161), // 清新绿成功色
            diff_add_bg: RgbaColor::from_rgba(166, 227, 161, 0.2), // 浅绿差异底色
            diff_remove_bg: RgbaColor::from_rgba(243, 139, 168, 0.2), // 浅红差异底色
        }
    }

    /// 浅色明亮调色板
    pub fn light() -> Self {
        Self {
            background: RgbaColor::from_rgb(248, 249, 250), // 明亮浅白底色
            surface: RgbaColor::from_rgb(255, 255, 255),    // 纯白面板色
            surface_elevated: RgbaColor::from_rgb(241, 243, 245), // 浅灰卡片色
            text_primary: RgbaColor::from_rgb(33, 37, 41),  // 墨黑正文色
            text_secondary: RgbaColor::from_rgb(108, 117, 125), // 辅助灰说明色
            border: RgbaColor::from_rgb(222, 226, 230),     // 浅灰分割线
            accent: RgbaColor::from_rgb(13, 110, 253),      // 经典蓝强调色
            danger: RgbaColor::from_rgb(220, 53, 69),       // 艳红危险色
            warning: RgbaColor::from_rgb(255, 193, 7),      // 琥珀黄警示色
            success: RgbaColor::from_rgb(25, 135, 84),      // 纯正绿成功色
            diff_add_bg: RgbaColor::from_rgba(25, 135, 84, 0.15),
            diff_remove_bg: RgbaColor::from_rgba(220, 53, 69, 0.15),
        }
    }

    /// 无障碍高对比度调色板 (符合 WCAG AAA 级最高对比标准)
    pub fn high_contrast() -> Self {
        Self {
            background: RgbaColor::from_rgb(0, 0, 0), // 纯黑背景
            surface: RgbaColor::from_rgb(0, 0, 0),    // 纯黑面板
            surface_elevated: RgbaColor::from_rgb(20, 20, 20), // 极深灰卡片
            text_primary: RgbaColor::from_rgb(255, 255, 255), // 纯白主字
            text_secondary: RgbaColor::from_rgb(255, 255, 255), // 纯白辅字（确保最高对比度）
            border: RgbaColor::from_rgb(255, 255, 255), // 纯白醒目边框
            accent: RgbaColor::from_rgb(255, 255, 0), // 高对比亮黄强调色
            danger: RgbaColor::from_rgb(255, 0, 0),   // 纯红高危色
            warning: RgbaColor::from_rgb(255, 255, 0), // 纯黄警示色
            success: RgbaColor::from_rgb(0, 255, 0),  // 纯绿成功色
            diff_add_bg: RgbaColor::from_rgba(0, 255, 0, 0.35),
            diff_remove_bg: RgbaColor::from_rgba(255, 0, 0, 0.35),
        }
    }

    /// 根据实际生效的主题获取对应的调色板
    pub fn for_theme(resolved: ResolvedTheme) -> Self {
        match resolved {
            ResolvedTheme::Dark => Self::dark(),
            ResolvedTheme::Light => Self::light(),
            ResolvedTheme::HighContrast => Self::high_contrast(),
        }
    }
}

/// 无障碍字体缩放比例
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontScale {
    /// 标准大小 (100%)
    #[default]
    Standard,
    /// 中等放大 (125%)
    Medium,
    /// 大字号 (150%)
    Large,
    /// 超大字号 (200%，视障友好)
    ExtraLarge,
}

impl FontScale {
    /// 获取缩放浮点系数
    pub fn factor(&self) -> f32 {
        match self {
            FontScale::Standard => 1.0,
            FontScale::Medium => 1.25,
            FontScale::Large => 1.5,
            FontScale::ExtraLarge => 2.0,
        }
    }
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

impl FontConfig {
    /// 计算缩放后的正文字号
    pub fn scaled_base_size(&self, scale: FontScale) -> u16 {
        (self.base_font_size as f32 * scale.factor()).round() as u16
    }

    /// 计算缩放后的标题字号
    pub fn scaled_title_size(&self, scale: FontScale) -> u16 {
        (self.title_font_size as f32 * scale.factor()).round() as u16
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
    fn test_theme_mode_default_and_resolve() {
        let theme = ThemeMode::default();
        assert_eq!(theme, ThemeMode::System);

        // System 跟随深色
        assert_eq!(
            theme.resolve(SystemColorScheme::PreferDark),
            ResolvedTheme::Dark
        );
        // System 跟随浅色
        assert_eq!(
            theme.resolve(SystemColorScheme::PreferLight),
            ResolvedTheme::Light
        );
        // System 未指定偏好默认深色
        assert_eq!(
            theme.resolve(SystemColorScheme::NoPreference),
            ResolvedTheme::Dark
        );

        // 手动模式不受系统偏好影响
        assert_eq!(
            ThemeMode::Light.resolve(SystemColorScheme::PreferDark),
            ResolvedTheme::Light
        );
        assert_eq!(
            ThemeMode::HighContrast.resolve(SystemColorScheme::PreferLight),
            ResolvedTheme::HighContrast
        );
    }

    #[test]
    fn test_rgba_color_hex_parsing() {
        let c1 = RgbaColor::from_hex("#1e1e2e").unwrap();
        assert_eq!(c1, RgbaColor::from_rgb(30, 30, 46));
        assert_eq!(c1.to_hex_string(), "#1e1e2e");

        let c2 = RgbaColor::from_hex("#fff").unwrap();
        assert_eq!(c2, RgbaColor::from_rgb(255, 255, 255));

        let c3 = RgbaColor::from_hex("00ff00").unwrap();
        assert_eq!(c3, RgbaColor::from_rgb(0, 255, 0));

        let err = RgbaColor::from_hex("invalid");
        assert!(err.is_err());
    }

    #[test]
    fn test_color_palettes() {
        let dark = ColorPalette::dark();
        assert_eq!(dark.background, RgbaColor::from_rgb(30, 30, 46));

        let light = ColorPalette::light();
        assert_eq!(light.background, RgbaColor::from_rgb(248, 249, 250));

        let hc = ColorPalette::high_contrast();
        assert_eq!(hc.background, RgbaColor::from_rgb(0, 0, 0));
        assert_eq!(hc.text_primary, RgbaColor::from_rgb(255, 255, 255));
    }

    #[test]
    fn test_font_scale_calculation() {
        let font_cfg = FontConfig::default();
        assert_eq!(font_cfg.scaled_base_size(FontScale::Standard), 14);
        assert_eq!(font_cfg.scaled_base_size(FontScale::Medium), 18);
        assert_eq!(font_cfg.scaled_base_size(FontScale::Large), 21);
        assert_eq!(font_cfg.scaled_base_size(FontScale::ExtraLarge), 28);

        assert_eq!(font_cfg.scaled_title_size(FontScale::Standard), 18);
        assert_eq!(font_cfg.scaled_title_size(FontScale::ExtraLarge), 36);
    }

    #[test]
    fn test_font_config_cjk_fallback() {
        let font_cfg = FontConfig::default();
        assert_eq!(font_cfg.font_families[0], "Noto Sans CJK SC");
        assert_eq!(font_cfg.font_families[1], "Source Han Sans SC");
        assert_eq!(font_cfg.font_families[2], "WenQuanYi Micro Hei");
    }

    #[test]
    fn test_window_config_hidpi_scaling() {
        let win_cfg = WindowConfig::default();
        assert!(win_cfg.prefer_wayland);

        let (w1, h1) = win_cfg.calculate_physical_size(1.0);
        assert_eq!(w1, 960);
        assert_eq!(h1, 640);

        let (w15, h15) = win_cfg.calculate_physical_size(1.5);
        assert_eq!(w15, 1440);
        assert_eq!(h15, 960);
    }
}
