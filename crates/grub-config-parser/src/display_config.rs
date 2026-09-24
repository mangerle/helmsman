//! 显示相关配置（主题路径、背景、颜色、分辨率）的类型化契约。
//!
//! # 设计原理
//! - **实现初衷**：`GRUB_GFXMODE` / `GRUB_GFXPAYLOAD` / `GRUB_BACKGROUND` / `GRUB_COLOR_*`
//!   在不同发行版上取值格式差异大，裸字符串极易写出无效模式导致黑屏。
//! - **核心优势**：分辨率采用 `宽x高[x色深]` 强校验，颜色采用 GRUB 语义色名/十六进制白名单，
//!   写回时仍走 AST 保留注释与引号。
//! - **代价与局限**：硬件实际可用模式依赖宿主机探测，本模块提供常见安全列表与格式校验，
//!   真实探测失败时调用方应回退 `auto`。

use crate::ast::GrubConfigFile;
use thiserror::Error;

/// 显示配置校验错误
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DisplayConfigError {
    /// 分辨率模式非法
    #[error("分辨率模式 '{mode}' 非法，原因: {reason}")]
    InvalidGfxMode {
        /// 被拒绝的模式
        mode: String,
        /// 拒绝原因
        reason: String,
    },
    /// 颜色值非法
    #[error("颜色值 '{color}' 非法，原因: {reason}")]
    InvalidColor {
        /// 被拒绝的颜色
        color: String,
        /// 拒绝原因
        reason: String,
    },
    /// 背景/主题路径非法
    #[error("路径 '{path}' 非法，原因: {reason}")]
    InvalidPath {
        /// 被拒绝的路径
        path: String,
        /// 拒绝原因
        reason: String,
    },
}

/// 分辨率模式（`GRUB_GFXMODE` / `GRUB_GFXPAYLOAD`）
///
/// 允许 `auto`、`keep` 或 `宽x高` / `宽x高x色深`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GfxMode(pub String);

impl GfxMode {
    /// 自动探测
    pub const AUTO: &str = "auto";
    /// 保持内核帧缓冲
    pub const KEEP: &str = "keep";

    /// 常见安全分辨率列表（探测失败时的回退候选）
    pub const COMMON_MODES: &'static [&'static str] = &[
        "1024x768",
        "1280x720",
        "1280x800",
        "1366x768",
        "1440x900",
        "1600x900",
        "1680x1050",
        "1920x1080",
        "2560x1440",
        "3840x2160",
    ];

    /// 校验并构造分辨率模式
    ///
    /// # Errors
    /// 当模式不是 auto/keep 且不符合 `宽x高[x色深]` 时返回 [`DisplayConfigError::InvalidGfxMode`]。
    pub fn parse(raw: &str) -> Result<Self, DisplayConfigError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DisplayConfigError::InvalidGfxMode {
                mode: raw.to_string(),
                reason: "分辨率模式不能为空".to_string(),
            });
        }
        if trimmed == Self::AUTO || trimmed == Self::KEEP {
            return Ok(GfxMode(trimmed.to_string()));
        }

        let parts: Vec<&str> = trimmed.split('x').collect();
        if !(2..=3).contains(&parts.len()) {
            return Err(DisplayConfigError::InvalidGfxMode {
                mode: raw.to_string(),
                reason: "格式应为 宽x高 或 宽x高x色深".to_string(),
            });
        }

        for part in &parts {
            let n: u32 = part
                .parse()
                .map_err(|_| DisplayConfigError::InvalidGfxMode {
                    mode: raw.to_string(),
                    reason: format!("分量 '{part}' 必须是正整数"),
                })?;
            if n == 0 {
                return Err(DisplayConfigError::InvalidGfxMode {
                    mode: raw.to_string(),
                    reason: "分量必须大于 0".to_string(),
                });
            }
        }

        // 宽高上限防止异常值（16K 以内）
        let w: u32 = parts[0].parse().unwrap_or(0);
        let h: u32 = parts[1].parse().unwrap_or(0);
        if w > 16384 || h > 16384 {
            return Err(DisplayConfigError::InvalidGfxMode {
                mode: raw.to_string(),
                reason: "分辨率不得超过 16384".to_string(),
            });
        }

        Ok(GfxMode(trimmed.to_string()))
    }

    /// 获取原始字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// GRUB 语义颜色（`fg/bg` 形态）
///
/// 示例：`white/black`、`black/light-gray`、`#ff0000/#000000`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrubColor(pub String);

impl GrubColor {
    /// GRUB 允许的语义色名
    pub const NAMED_COLORS: &'static [&'static str] = &[
        "black",
        "blue",
        "green",
        "cyan",
        "red",
        "magenta",
        "brown",
        "light-gray",
        "dark-gray",
        "light-blue",
        "light-green",
        "light-cyan",
        "light-red",
        "light-magenta",
        "yellow",
        "white",
    ];

    /// 校验并构造颜色对
    ///
    /// # Errors
    /// 当色名/十六进制格式非法时返回 [`DisplayConfigError::InvalidColor`]。
    pub fn parse(raw: &str) -> Result<Self, DisplayConfigError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || !trimmed.contains('/') {
            return Err(DisplayConfigError::InvalidColor {
                color: raw.to_string(),
                reason: "颜色格式应为 前景/背景".to_string(),
            });
        }
        let mut parts = trimmed.splitn(2, '/');
        let fg = parts.next().unwrap_or("").trim();
        let bg = parts.next().unwrap_or("").trim();
        Self::validate_component(fg, raw)?;
        Self::validate_component(bg, raw)?;
        Ok(GrubColor(trimmed.to_string()))
    }

    fn validate_component(component: &str, raw: &str) -> Result<(), DisplayConfigError> {
        if Self::NAMED_COLORS.contains(&component) {
            return Ok(());
        }
        // 十六进制 #rrggbb
        if let Some(hex) = component.strip_prefix('#')
            && hex.len() == 6
            && hex.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Ok(());
        }
        Err(DisplayConfigError::InvalidColor {
            color: raw.to_string(),
            reason: format!("色名或十六进制分量 '{component}' 不受支持"),
        })
    }

    /// 获取原始字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 校验背景/主题路径：禁止控制字符与 shell 元字符
///
/// # Errors
/// 当路径为空或含危险字符时返回 [`DisplayConfigError::InvalidPath`]。
pub fn validate_display_path(path: &str) -> Result<(), DisplayConfigError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(DisplayConfigError::InvalidPath {
            path: path.to_string(),
            reason: "路径不能为空".to_string(),
        });
    }
    if trimmed.len() > 512 {
        return Err(DisplayConfigError::InvalidPath {
            path: path.to_string(),
            reason: "路径不得超过 512 字符".to_string(),
        });
    }
    let bad = trimmed.chars().any(|c| {
        c.is_control()
            || matches!(
                c,
                '\n' | '\r'
                    | '\0'
                    | '\''
                    | '"'
                    | '`'
                    | '$'
                    | ';'
                    | '&'
                    | '|'
                    | '<'
                    | '>'
                    | '('
                    | ')'
                    | '{'
                    | '}'
                    | '\\'
            )
    });
    if bad {
        return Err(DisplayConfigError::InvalidPath {
            path: path.to_string(),
            reason: "禁止包含换行、引号或 shell 元字符".to_string(),
        });
    }
    Ok(())
}

/// 读取/写入显示相关键
pub mod keys {
    use super::*;

    /// 读取 `GRUB_GFXMODE`
    pub fn get_gfxmode(config: &GrubConfigFile) -> Result<Option<GfxMode>, DisplayConfigError> {
        match config.get("GRUB_GFXMODE") {
            Some(raw) => GfxMode::parse(raw).map(Some),
            None => Ok(None),
        }
    }

    /// 写入 `GRUB_GFXMODE`；传 `None` 表示移除键（跟随发行版默认）
    pub fn set_gfxmode(
        config: &mut GrubConfigFile,
        mode: Option<&GfxMode>,
    ) -> Result<(), DisplayConfigError> {
        match mode {
            Some(m) => {
                GfxMode::parse(m.as_str())?;
                config.set("GRUB_GFXMODE", m.as_str());
            }
            None => {
                config.remove("GRUB_GFXMODE");
            }
        }
        Ok(())
    }

    /// 读取 `GRUB_GFXPAYLOAD`
    pub fn get_gfxpayload(config: &GrubConfigFile) -> Result<Option<GfxMode>, DisplayConfigError> {
        match config.get("GRUB_GFXPAYLOAD") {
            Some(raw) => GfxMode::parse(raw).map(Some),
            None => Ok(None),
        }
    }

    /// 写入 `GRUB_GFXPAYLOAD`；传 `None` 表示移除键
    pub fn set_gfxpayload(
        config: &mut GrubConfigFile,
        mode: Option<&GfxMode>,
    ) -> Result<(), DisplayConfigError> {
        match mode {
            Some(m) => {
                GfxMode::parse(m.as_str())?;
                config.set("GRUB_GFXPAYLOAD", m.as_str());
            }
            None => {
                config.remove("GRUB_GFXPAYLOAD");
            }
        }
        Ok(())
    }

    /// 写入背景路径；`None` 表示移除
    pub fn set_background(
        config: &mut GrubConfigFile,
        path: Option<&str>,
    ) -> Result<(), DisplayConfigError> {
        match path.map(str::trim) {
            Some(p) if !p.is_empty() => {
                validate_display_path(p)?;
                config.set("GRUB_BACKGROUND", p);
            }
            _ => {
                config.remove("GRUB_BACKGROUND");
            }
        }
        Ok(())
    }

    /// 写入终端颜色对；`None` 表示移除
    pub fn set_colors(
        config: &mut GrubConfigFile,
        normal: Option<&GrubColor>,
        highlight: Option<&GrubColor>,
    ) -> Result<(), DisplayConfigError> {
        match normal {
            Some(c) => {
                GrubColor::parse(c.as_str())?;
                config.set("GRUB_COLOR_NORMAL", c.as_str());
            }
            None => {
                config.remove("GRUB_COLOR_NORMAL");
            }
        }
        match highlight {
            Some(c) => {
                GrubColor::parse(c.as_str())?;
                config.set("GRUB_COLOR_HIGHLIGHT", c.as_str());
            }
            None => {
                config.remove("GRUB_COLOR_HIGHLIGHT");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_grub_config;

    #[test]
    fn test_gfxmode_parse() {
        assert_eq!(GfxMode::parse("auto").unwrap().as_str(), "auto");
        assert_eq!(GfxMode::parse("keep").unwrap().as_str(), "keep");
        assert_eq!(GfxMode::parse("1920x1080").unwrap().as_str(), "1920x1080");
        assert_eq!(
            GfxMode::parse("1024x768x32").unwrap().as_str(),
            "1024x768x32"
        );
        assert!(GfxMode::parse("").is_err());
        assert!(GfxMode::parse("1920").is_err());
        assert!(GfxMode::parse("0x1080").is_err());
        assert!(GfxMode::parse("99999x1080").is_err());
    }

    #[test]
    fn test_grub_color_parse() {
        assert!(GrubColor::parse("white/black").is_ok());
        assert!(GrubColor::parse("light-blue/black").is_ok());
        assert!(GrubColor::parse("#ff0000/#000000").is_ok());
        assert!(GrubColor::parse("white").is_err());
        assert!(GrubColor::parse("rainbow/black").is_err());
        assert!(GrubColor::parse("#gg0000/black").is_err());
    }

    #[test]
    fn test_set_display_keys_preserve_format() {
        let mut config = parse_grub_config("# 显示配置\nGRUB_TIMEOUT=5\n");
        keys::set_gfxmode(&mut config, Some(&GfxMode::parse("1920x1080").unwrap())).unwrap();
        keys::set_gfxpayload(&mut config, Some(&GfxMode::parse("keep").unwrap())).unwrap();
        keys::set_background(&mut config, Some("/boot/grub/bg.png")).unwrap();
        keys::set_colors(
            &mut config,
            Some(&GrubColor::parse("white/black").unwrap()),
            Some(&GrubColor::parse("black/light-gray").unwrap()),
        )
        .unwrap();

        let text = config.serialize();
        assert!(text.contains("# 显示配置"));
        assert!(text.contains("GRUB_GFXMODE=\"1920x1080\""));
        assert!(text.contains("GRUB_GFXPAYLOAD=\"keep\""));
        assert!(text.contains("GRUB_BACKGROUND=\"/boot/grub/bg.png\""));
        assert!(text.contains("GRUB_COLOR_NORMAL=\"white/black\""));

        // 移除后键消失
        keys::set_gfxmode(&mut config, None).unwrap();
        assert!(config.get("GRUB_GFXMODE").is_none());
    }

    #[test]
    fn test_common_modes_are_valid() {
        for mode in GfxMode::COMMON_MODES {
            assert!(GfxMode::parse(mode).is_ok(), "常见模式应合法: {mode}");
        }
    }

    #[test]
    fn test_validate_display_path() {
        assert!(validate_display_path("/boot/grub/bg.png").is_ok());
        assert!(validate_display_path("").is_err());
        assert!(validate_display_path("/boot/$(evil).png").is_err());
    }
}
