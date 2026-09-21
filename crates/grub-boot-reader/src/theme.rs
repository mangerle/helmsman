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
