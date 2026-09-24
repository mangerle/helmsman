use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 尺寸与坐标维度（支持绝对像素与相对百分比）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThemeDimension {
    /// 绝对像素值 (如 "400", "-50")
    Pixels(i32),
    /// 相对父容器/屏幕的百分比 (如 "50%", "100%")
    Percent(f32),
}

impl ThemeDimension {
    /// 从字符串解析尺寸维度（支持 "50%", "400px", "300"）
    pub fn parse(s: &str) -> Self {
        let trimmed = s.trim().trim_matches('"');
        if let Some(pct_str) = trimmed.strip_suffix('%')
            && let Ok(val) = pct_str.trim().parse::<f32>()
        {
            return ThemeDimension::Percent(val);
        }
        let px_str = trimmed.strip_suffix("px").unwrap_or(trimmed);
        if let Ok(val) = px_str.trim().parse::<i32>() {
            ThemeDimension::Pixels(val)
        } else {
            ThemeDimension::Pixels(0)
        }
    }

    /// 根据总像素计算具体的绝对像素值
    pub fn resolve_pixels(&self, total_pixels: u32) -> i32 {
        match self {
            ThemeDimension::Pixels(px) => *px,
            ThemeDimension::Percent(pct) => ((total_pixels as f32) * (pct / 100.0)).round() as i32,
        }
    }
}

/// 引导菜单列表框组件属性
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BootMenuComponent {
    pub left: Option<ThemeDimension>,
    pub top: Option<ThemeDimension>,
    pub width: Option<ThemeDimension>,
    pub height: Option<ThemeDimension>,
    pub item_font: Option<String>,
    pub item_color: Option<String>,
    pub selected_item_color: Option<String>,
    pub item_height: Option<u32>,
    pub item_padding: Option<u32>,
    pub item_spacing: Option<u32>,
    pub icon_width: Option<u32>,
    pub icon_height: Option<u32>,
}

/// 倒计时进度条组件属性
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProgressBarComponent {
    pub id: Option<String>,
    pub left: Option<ThemeDimension>,
    pub top: Option<ThemeDimension>,
    pub width: Option<ThemeDimension>,
    pub height: Option<ThemeDimension>,
    pub fg_color: Option<String>,
    pub bg_color: Option<String>,
    pub border_color: Option<String>,
    pub text: Option<String>,
    pub font: Option<String>,
    pub text_color: Option<String>,
}

/// 文本标签组件属性
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LabelComponent {
    pub id: Option<String>,
    pub text: Option<String>,
    pub font: Option<String>,
    pub color: Option<String>,
    pub align: Option<String>,
    pub left: Option<ThemeDimension>,
    pub top: Option<ThemeDimension>,
    pub width: Option<ThemeDimension>,
    pub height: Option<ThemeDimension>,
}

/// 静态图片组件属性
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImageComponent {
    pub file: Option<String>,
    pub left: Option<ThemeDimension>,
    pub top: Option<ThemeDimension>,
    pub width: Option<ThemeDimension>,
    pub height: Option<ThemeDimension>,
}

/// GRUB 主题组件枚举
#[derive(Debug, Clone, PartialEq)]
pub enum ThemeComponent {
    /// 引导条目菜单框
    BootMenu(BootMenuComponent),
    /// 倒计时进度条
    ProgressBar(ProgressBarComponent),
    /// 静态标签
    Label(LabelComponent),
    /// 静态贴图
    Image(ImageComponent),
    /// 其他未建模组件（防止解析中断）
    Other {
        component_type: String,
        properties: HashMap<String, String>,
    },
}

/// GRUB 主题完整定义模型
///
/// # 设计原理
/// - **实现初衷**：GRUB 官方通过 `theme.txt` 控制开机视觉排版。本结构体解析并持有主题的
///   全局背景、字体颜色以及各子组件的几何坐标与外观属性。
/// - **核心优势**：将松散的 GRUB 文本规则解析为强类型抽象，供 UI 预览引擎与系统配置层安全消费。
#[derive(Debug, Clone, PartialEq)]
pub struct GrubThemeDefinition {
    /// 主题显示名称（通常由所在目录名派生）
    pub name: String,
    /// `theme.txt` 文件的绝对路径
    pub theme_file_path: PathBuf,
    /// 主题所在根目录
    pub theme_dir: PathBuf,
    /// 全局标题文字
    pub title_text: Option<String>,
    /// 全局桌面背景图片路径（已规范为基于主题目录的绝对路径）
    pub desktop_image: Option<PathBuf>,
    /// 全局桌面背景纯色值 (如 "#000000")
    pub desktop_color: Option<String>,
    /// 解析出的组件列表
    pub components: Vec<ThemeComponent>,
}

impl GrubThemeDefinition {
    /// 获取引导菜单列表框组件定义（若存在）
    pub fn boot_menu(&self) -> Option<&BootMenuComponent> {
        self.components.iter().find_map(|c| match c {
            ThemeComponent::BootMenu(m) => Some(m),
            _ => None,
        })
    }

    /// 获取倒计时进度条组件定义（若存在）
    pub fn progress_bar(&self) -> Option<&ProgressBarComponent> {
        self.components.iter().find_map(|c| match c {
            ThemeComponent::ProgressBar(p) => Some(p),
            _ => None,
        })
    }

    /// 校验主题定义的完整性与资源引用
    ///
    /// # Errors
    /// 当缺少背景/组件、引用图不存在或颜色非法时返回 [`ThemeValidationError`]。
    pub fn validate(&self) -> Result<(), ThemeValidationError> {
        // 至少要有背景图、背景色或任一组件，否则开机菜单将一片空白
        let has_visual = self.desktop_image.is_some()
            || self.desktop_color.is_some()
            || !self.components.is_empty();
        if !has_visual {
            return Err(ThemeValidationError::MissingVisual {
                theme: self.name.clone(),
                reason: "未配置 desktop-image、desktop-color 或任何组件".to_string(),
            });
        }

        if let Some(ref img) = self.desktop_image
            && !img.is_file()
        {
            return Err(ThemeValidationError::ResourceMissing {
                theme: self.name.clone(),
                path: img.display().to_string(),
            });
        }

        if let Some(ref color) = self.desktop_color
            && !is_plausible_theme_color(color)
        {
            return Err(ThemeValidationError::InvalidColor {
                theme: self.name.clone(),
                color: color.clone(),
            });
        }

        Ok(())
    }

    /// 导出无 GUI 预览摘要
    pub fn preview_summary(&self) -> ThemePreviewSummary {
        let menu = self.boot_menu();
        ThemePreviewSummary {
            name: self.name.clone(),
            title_text: self.title_text.clone(),
            desktop_color: self.desktop_color.clone(),
            has_desktop_image: self.desktop_image.is_some(),
            desktop_image_name: self
                .desktop_image
                .as_ref()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())),
            item_color: menu.and_then(|m| m.item_color.clone()),
            selected_item_color: menu.and_then(|m| m.selected_item_color.clone()),
            component_count: self.components.len(),
            has_boot_menu: menu.is_some(),
            has_progress_bar: self.progress_bar().is_some(),
        }
    }
}

/// 主题定义校验错误
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeValidationError {
    /// 缺少关键视觉定义
    #[error("主题 '{theme}' 缺少关键视觉定义，原因: {reason}")]
    MissingVisual {
        /// 主题名
        theme: String,
        /// 原因
        reason: String,
    },
    /// 引用的资源文件不存在
    #[error("主题 '{theme}' 引用资源缺失，路径: {path}")]
    ResourceMissing {
        /// 主题名
        theme: String,
        /// 缺失路径
        path: String,
    },
    /// 颜色值格式非法
    #[error("主题 '{theme}' 颜色值 '{color}' 非法")]
    InvalidColor {
        /// 主题名
        theme: String,
        /// 颜色值
        color: String,
    },
}

/// 主题预览摘要（无 GUI 环境下可导出的结构化预览数据）
///
/// # 设计原理
/// - **实现初衷**：无图形界面时仍需向用户/脚本展示主题“长什么样”的关键视觉信息。
/// - **核心优势**：只暴露颜色、标题、组件计数等轻量元数据，不携带大图二进制。
/// - **代价与局限**：不是像素级渲染结果；完整视觉预览仍需 UI 渲染层。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ThemePreviewSummary {
    /// 主题名称
    pub name: String,
    /// 全局标题文字
    pub title_text: Option<String>,
    /// 桌面背景纯色
    pub desktop_color: Option<String>,
    /// 是否配置了背景图
    pub has_desktop_image: bool,
    /// 背景图文件名（不含目录，便于展示）
    pub desktop_image_name: Option<String>,
    /// 菜单普通项颜色
    pub item_color: Option<String>,
    /// 菜单高亮项颜色
    pub selected_item_color: Option<String>,
    /// 组件总数
    pub component_count: usize,
    /// 是否包含 boot_menu 组件
    pub has_boot_menu: bool,
    /// 是否包含 progress_bar 组件
    pub has_progress_bar: bool,
}

/// 宽松校验主题颜色：允许 `#rrggbb`、`#rgb` 与 GRUB 语义色名
fn is_plausible_theme_color(color: &str) -> bool {
    let c = color.trim();
    if c.is_empty() || c.len() > 32 {
        return false;
    }
    if let Some(hex) = c.strip_prefix('#') {
        return !hex.is_empty()
            && hex.len().is_multiple_of(3)
            && hex.chars().all(|ch| ch.is_ascii_hexdigit());
    }
    // 语义色名（含连字符）
    c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

/// 主题元数据摘要（用于主题列表快速扫描展示）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrubThemeMeta {
    /// 主题名称
    pub name: String,
    /// 主题所在目录绝对路径
    pub dir_path: PathBuf,
    /// `theme.txt` 绝对路径
    pub theme_txt_path: PathBuf,
    /// 是否存在背景图片
    pub has_desktop_image: bool,
    /// 预览缩略图路径（如 preview.png 或 screenshot.png）
    pub preview_image_path: Option<PathBuf>,
}

/// 解析 `theme.txt` 文件内容为 `GrubThemeDefinition`
pub fn parse_grub_theme(content: &str, theme_file_path: &Path) -> GrubThemeDefinition {
    let theme_dir = theme_file_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let name = theme_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "default".to_string());

    let mut title_text = None;
    let mut desktop_image = None;
    let mut desktop_color = None;
    let mut components = Vec::new();

    let mut lines = content.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(component_decl) = trimmed.strip_prefix('+') {
            // 解析组件块，形如 "+ boot_menu {"
            let (comp_type, mut props) = parse_component_block(component_decl, &mut lines);
            components.push(build_component(&comp_type, &mut props));
        } else if let Some((k, v)) = parse_key_value(trimmed) {
            match k.as_str() {
                "title-text" => title_text = Some(v),
                "desktop-image" => {
                    let img_path = theme_dir.join(&v);
                    desktop_image = Some(img_path);
                }
                "desktop-color" => desktop_color = Some(v),
                _ => {}
            }
        }
    }

    GrubThemeDefinition {
        name,
        theme_file_path: theme_file_path.to_path_buf(),
        theme_dir,
        title_text,
        desktop_image,
        desktop_color,
        components,
    }
}

/// 解析单行键值对（支持 `key = "value"`, `key: "value"`, `key = value`）
fn parse_key_value(line: &str) -> Option<(String, String)> {
    let separator_idx = line.find('=').or_else(|| line.find(':'))?;
    let key = line[..separator_idx].trim().to_lowercase();
    let val_part = line[separator_idx + 1..].trim();
    let val = val_part
        .strip_suffix(';')
        .unwrap_or(val_part)
        .trim()
        .trim_matches('"')
        .to_string();

    if key.is_empty() {
        None
    } else {
        Some((key, val))
    }
}

/// 解析组件大括号代码块
fn parse_component_block<'a, I>(
    declaration: &str,
    lines: &mut I,
) -> (String, HashMap<String, String>)
where
    I: Iterator<Item = &'a str>,
{
    let comp_type = declaration
        .trim_start_matches('+')
        .split('{')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();

    let mut props = HashMap::new();
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.contains('}') {
            break;
        }
        if let Some((k, v)) = parse_key_value(trimmed) {
            props.insert(k, v);
        }
    }
    (comp_type, props)
}

/// 根据组件类型与属性映射表构建强类型组件结构
fn build_component(comp_type: &str, props: &mut HashMap<String, String>) -> ThemeComponent {
    match comp_type {
        "boot_menu" => ThemeComponent::BootMenu(BootMenuComponent {
            left: props.remove("left").map(|s| ThemeDimension::parse(&s)),
            top: props.remove("top").map(|s| ThemeDimension::parse(&s)),
            width: props.remove("width").map(|s| ThemeDimension::parse(&s)),
            height: props.remove("height").map(|s| ThemeDimension::parse(&s)),
            item_font: props.remove("item_font"),
            item_color: props.remove("item_color"),
            selected_item_color: props.remove("selected_item_color"),
            item_height: props.remove("item_height").and_then(|s| s.parse().ok()),
            item_padding: props.remove("item_padding").and_then(|s| s.parse().ok()),
            item_spacing: props.remove("item_spacing").and_then(|s| s.parse().ok()),
            icon_width: props.remove("icon_width").and_then(|s| s.parse().ok()),
            icon_height: props.remove("icon_height").and_then(|s| s.parse().ok()),
        }),
        "progress_bar" | "circular_progress" => ThemeComponent::ProgressBar(ProgressBarComponent {
            id: props.remove("id"),
            left: props.remove("left").map(|s| ThemeDimension::parse(&s)),
            top: props.remove("top").map(|s| ThemeDimension::parse(&s)),
            width: props.remove("width").map(|s| ThemeDimension::parse(&s)),
            height: props.remove("height").map(|s| ThemeDimension::parse(&s)),
            fg_color: props
                .remove("fg_color")
                .or_else(|| props.remove("highlight_color")),
            bg_color: props.remove("bg_color"),
            border_color: props.remove("border_color"),
            text: props.remove("text"),
            font: props.remove("font"),
            text_color: props.remove("text_color"),
        }),
        "label" => ThemeComponent::Label(LabelComponent {
            id: props.remove("id"),
            text: props.remove("text"),
            font: props.remove("font"),
            color: props.remove("color"),
            align: props.remove("align"),
            left: props.remove("left").map(|s| ThemeDimension::parse(&s)),
            top: props.remove("top").map(|s| ThemeDimension::parse(&s)),
            width: props.remove("width").map(|s| ThemeDimension::parse(&s)),
            height: props.remove("height").map(|s| ThemeDimension::parse(&s)),
        }),
        "image" => ThemeComponent::Image(ImageComponent {
            file: props.remove("file"),
            left: props.remove("left").map(|s| ThemeDimension::parse(&s)),
            top: props.remove("top").map(|s| ThemeDimension::parse(&s)),
            width: props.remove("width").map(|s| ThemeDimension::parse(&s)),
            height: props.remove("height").map(|s| ThemeDimension::parse(&s)),
        }),
        _ => ThemeComponent::Other {
            component_type: comp_type.to_string(),
            properties: props.clone(),
        },
    }
}

/// 扫描指定路径列表下的所有可用 GRUB 主题
pub fn scan_available_themes(search_paths: &[&Path]) -> Vec<GrubThemeMeta> {
    let mut results = Vec::new();
    for base in search_paths {
        if !base.is_dir() {
            continue;
        }
        let read_dir = match fs::read_dir(base) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        for entry in read_dir.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let theme_txt = path.join("theme.txt");
            if theme_txt.is_file() {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "unknown".to_string());

                let has_desktop_image = path.join("background.png").is_file()
                    || path.join("background.jpg").is_file()
                    || path.join("background.jpeg").is_file();

                let preview_candidates = ["preview.png", "screenshot.png", "background.png"];
                let preview_image_path = preview_candidates
                    .iter()
                    .map(|c| path.join(c))
                    .find(|p| p.is_file());

                results.push(GrubThemeMeta {
                    name,
                    dir_path: path,
                    theme_txt_path: theme_txt,
                    has_desktop_image,
                    preview_image_path,
                });
            }
        }
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_theme_content() -> &'static str {
        r##"
# 测试主题
title-text = "Helmsman 测试"
desktop-color = "#1e1e2e"
desktop-image = "background.png"

+ boot_menu {
  left = 20%
  top = 30%
  width = 60%
  item_color = "white"
  selected_item_color = "black/light-gray"
}

+ progress_bar {
  id = "__timeout__"
  left = 30%
  top = 80%
  width = 40%
  height = 20
}
"##
    }

    #[test]
    fn test_theme_validate_and_preview_summary() {
        let dir = std::env::temp_dir().join("helmsman_theme_validate");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let theme_txt = dir.join("theme.txt");
        fs::write(&theme_txt, sample_theme_content()).unwrap();
        // 声明的背景图应存在才通过校验
        fs::write(dir.join("background.png"), b"fake").unwrap();

        let def = parse_grub_theme(sample_theme_content(), &theme_txt);
        def.validate().expect("完整主题应通过校验");

        let summary = def.preview_summary();
        assert_eq!(summary.title_text.as_deref(), Some("Helmsman 测试"));
        assert!(summary.has_desktop_image);
        assert!(summary.has_boot_menu);
        assert!(summary.has_progress_bar);
        assert_eq!(summary.component_count, 2);
        assert_eq!(summary.desktop_color.as_deref(), Some("#1e1e2e"));
    }

    #[test]
    fn test_theme_validate_missing_resource() {
        let dir = std::env::temp_dir().join("helmsman_theme_missing");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let theme_txt = dir.join("theme.txt");
        fs::write(&theme_txt, sample_theme_content()).unwrap();
        // 故意不创建 background.png

        let def = parse_grub_theme(sample_theme_content(), &theme_txt);
        assert!(matches!(
            def.validate(),
            Err(ThemeValidationError::ResourceMissing { .. })
        ));
    }

    #[test]
    fn test_theme_validate_empty_definition() {
        let theme_txt = Path::new("/tmp/empty-theme/theme.txt");
        let def = parse_grub_theme("# 空主题\n", theme_txt);
        assert!(matches!(
            def.validate(),
            Err(ThemeValidationError::MissingVisual { .. })
        ));
    }
}
