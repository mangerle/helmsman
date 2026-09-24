//! `/etc/default/grub` 中与引导默认项、倒计时及菜单显示风格相关的类型化契约。
//!
//! # 设计原理
//! - **实现初衷**：`GRUB_DEFAULT` / `GRUB_TIMEOUT` / `GRUB_TIMEOUT_STYLE` 是用户最高频修改的
//!   三项配置，若各调用点各自拼接裸字符串，极易写出非法值或破坏原有引号风格。
//!   本模块将上述键收敛为强类型枚举与新类型，解析与序列化集中在边界完成。
//! - **核心优势**：非法状态不可表示（如倒计时显示风格仅允许三种枚举值）；
//!   写回时复用 [`GrubConfigFile::set`]，完整保留注释、键序与原有引号类型。
//! - **代价与局限**：仅覆盖三项核心引导默认键；内核命令行、主题等由其他模块负责。

use crate::ast::GrubConfigFile;
use thiserror::Error;

/// `GRUB_TIMEOUT_STYLE` 合法取值
///
/// # 设计原理
/// - **实现初衷**：避免将用户输入的自由字符串直接写入配置导致 GRUB 解析异常。
/// - **代价与局限**：不保留 GRUB 未来可能新增的扩展风格，未知值在解析边界被拒绝。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutStyle {
    /// 显示完整菜单并倒计时
    Menu,
    /// 隐藏菜单，超时后直接启动默认项
    Hidden,
    /// 显示倒计时提示但不展开完整菜单
    Countdown,
}

impl TimeoutStyle {
    /// 序列化为 GRUB 配置取值
    pub const fn as_str(self) -> &'static str {
        match self {
            TimeoutStyle::Menu => "menu",
            TimeoutStyle::Hidden => "hidden",
            TimeoutStyle::Countdown => "countdown",
        }
    }

    /// 从 GRUB 配置原始字符串解析
    ///
    /// # Errors
    /// 当字符串不是 `menu` / `hidden` / `countdown` 之一时返回 [`BootKeyError::InvalidTimeoutStyle`]。
    pub fn parse(raw: &str) -> Result<Self, BootKeyError> {
        match raw.trim() {
            "menu" => Ok(TimeoutStyle::Menu),
            "hidden" => Ok(TimeoutStyle::Hidden),
            "countdown" => Ok(TimeoutStyle::Countdown),
            other => Err(BootKeyError::InvalidTimeoutStyle {
                raw: other.to_string(),
                reason: "允许值仅为 menu / hidden / countdown".to_string(),
            }),
        }
    }
}

/// 倒计时秒数新类型
///
/// # 设计原理
/// - **实现初衷**：`GRUB_TIMEOUT` 允许 `-1`（无限等待）与非负秒数，裸 `i32` 缺少语义标注，
///   容易与毫秒、无符号超时等概念混淆。
/// - **代价与局限**：上界按 GRUB 实际可用范围约束为 86400 秒（一天），极端场景需放宽时应同步更新校验。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimeoutSeconds(pub i32);

impl TimeoutSeconds {
    /// 无限等待（不自动启动）
    pub const INFINITE: TimeoutSeconds = TimeoutSeconds(-1);
    /// 允许的最大有限超时秒数
    pub const MAX_FINITE: i32 = 86_400;

    /// 校验并构造倒计时秒数
    ///
    /// # Errors
    /// 当取值小于 -1 或大于 [`Self::MAX_FINITE`] 时返回 [`BootKeyError::InvalidTimeout`]。
    pub fn try_new(seconds: i32) -> Result<Self, BootKeyError> {
        if !(-1..=Self::MAX_FINITE).contains(&seconds) {
            return Err(BootKeyError::InvalidTimeout {
                raw: seconds.to_string(),
                reason: format!(
                    "倒计时秒数必须位于 -1（无限等待）至 {} 之间",
                    Self::MAX_FINITE
                ),
            });
        }
        Ok(TimeoutSeconds(seconds))
    }

    /// 从原始字符串解析
    ///
    /// # Errors
    /// 当字符串不是合法整数或超出范围时返回 [`BootKeyError::InvalidTimeout`]。
    pub fn parse(raw: &str) -> Result<Self, BootKeyError> {
        let trimmed = raw.trim();
        let parsed = trimmed
            .parse::<i32>()
            .map_err(|e| BootKeyError::InvalidTimeout {
                raw: raw.to_string(),
                reason: format!("必须为十进制整数: {e}"),
            })?;
        Self::try_new(parsed)
    }

    /// 获取秒数原值
    pub const fn as_i32(self) -> i32 {
        self.0
    }

    /// 是否为无限等待
    pub const fn is_infinite(self) -> bool {
        self.0 < 0
    }
}

/// `GRUB_DEFAULT` 语义取值
///
/// # 设计原理
/// - **实现初衷**：README 约定采用原生 `GRUB_DEFAULT` 语义索引实现无损排序，
///   必须区分「菜单逻辑序号」「记忆上次启动」与「标题/层级路径」三种合法形态，
///   避免将任意字符串误当作索引写入。
/// - **核心优势**：语义索引不改动发行版脚本，内核升级后菜单顺序变化仍可被 GRUB 正确解析。
/// - **代价与局限**：纯序号在菜单结构大幅变动后可能指向错误条目，重要场景应优先使用
///   [`DefaultEntry::Title`] 或 `saved` 记忆模式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultEntry {
    /// 菜单扁平逻辑序号（从 0 开始）
    Index(u32),
    /// 记忆上次成功启动项（配合 `GRUB_SAVEDEFAULT`）
    Saved,
    /// 条目标题或子菜单层级路径（形如 `Advanced options>Ubuntu`）
    Title(String),
}

impl DefaultEntry {
    /// 序列化为 `GRUB_DEFAULT` 配置原值
    pub fn as_config_value(&self) -> String {
        match self {
            DefaultEntry::Index(idx) => idx.to_string(),
            DefaultEntry::Saved => "saved".to_string(),
            DefaultEntry::Title(title) => title.clone(),
        }
    }

    /// 从 `GRUB_DEFAULT` 原始字符串解析
    ///
    /// # Errors
    /// 当字符串为空或索引形态包含非法数字时返回 [`BootKeyError::InvalidDefaultEntry`]。
    pub fn parse(raw: &str) -> Result<Self, BootKeyError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(BootKeyError::InvalidDefaultEntry {
                raw: raw.to_string(),
                reason: "默认启动项不能为空".to_string(),
            });
        }

        if trimmed.eq_ignore_ascii_case("saved") {
            return Ok(DefaultEntry::Saved);
        }

        // 纯数字按语义索引解析；含空格或层级符号的按标题/路径解析
        if trimmed.chars().all(|c| c.is_ascii_digit()) {
            let idx = trimmed
                .parse::<u32>()
                .map_err(|e| BootKeyError::InvalidDefaultEntry {
                    raw: raw.to_string(),
                    reason: format!("菜单序号超出可表示范围: {e}"),
                })?;
            return Ok(DefaultEntry::Index(idx));
        }

        if trimmed.contains('\n') || trimmed.contains('\r') || trimmed.contains('\0') {
            return Err(BootKeyError::InvalidDefaultEntry {
                raw: raw.to_string(),
                reason: "标题或路径禁止包含换行与空字符".to_string(),
            });
        }

        Ok(DefaultEntry::Title(trimmed.to_string()))
    }

    /// 是否与给定原始配置值语义等价
    pub fn matches_raw(&self, raw: &str) -> bool {
        match DefaultEntry::parse(raw) {
            Ok(other) => self == &other,
            Err(_) => self.as_config_value() == raw.trim(),
        }
    }
}

/// 引导默认项键解析与写入错误
///
/// # 设计原理
/// - **实现初衷**：边界解析失败时必须给出可定位的中文原因，禁止静默回退为任意默认值。
/// - **代价与局限**：错误变体按键区分，调用方需逐项匹配处理。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BootKeyError {
    /// 倒计时秒数非法
    #[error("倒计时秒数 '{raw}' 非法，原因: {reason}")]
    InvalidTimeout {
        /// 被拒绝的原始值
        raw: String,
        /// 拒绝原因
        reason: String,
    },
    /// 倒计时显示风格非法
    #[error("倒计时显示风格 '{raw}' 非法，原因: {reason}")]
    InvalidTimeoutStyle {
        /// 被拒绝的原始值
        raw: String,
        /// 拒绝原因
        reason: String,
    },
    /// 默认启动项取值非法
    #[error("默认启动项 '{raw}' 非法，原因: {reason}")]
    InvalidDefaultEntry {
        /// 被拒绝的原始值
        raw: String,
        /// 拒绝原因
        reason: String,
    },
}

/// 读取倒计时秒数（缺省视为 5 秒）
///
/// # Errors
/// 当已配置值非法时返回 [`BootKeyError::InvalidTimeout`]。
pub fn get_timeout(config: &GrubConfigFile) -> Result<TimeoutSeconds, BootKeyError> {
    match config.get("GRUB_TIMEOUT") {
        Some(raw) => TimeoutSeconds::parse(raw),
        None => Ok(TimeoutSeconds(5)),
    }
}

/// 写入倒计时秒数（保留原有引号与行尾注释）
///
/// # Errors
/// 当传入秒数超出合法范围时返回 [`BootKeyError::InvalidTimeout`]。
pub fn set_timeout(
    config: &mut GrubConfigFile,
    timeout: TimeoutSeconds,
) -> Result<(), BootKeyError> {
    TimeoutSeconds::try_new(timeout.0)?;
    config.set("GRUB_TIMEOUT", &timeout.0.to_string());
    Ok(())
}

/// 读取倒计时显示风格（缺省视为 menu）
///
/// # Errors
/// 当已配置值非法时返回 [`BootKeyError::InvalidTimeoutStyle`]。
pub fn get_timeout_style(config: &GrubConfigFile) -> Result<TimeoutStyle, BootKeyError> {
    match config.get("GRUB_TIMEOUT_STYLE") {
        Some(raw) => TimeoutStyle::parse(raw),
        None => Ok(TimeoutStyle::Menu),
    }
}

/// 写入倒计时显示风格（保留原有引号与行尾注释）
///
/// # Errors
/// 当枚举序列化异常时返回 [`BootKeyError::InvalidTimeoutStyle`]（当前实现不会触发，预留契约）。
pub fn set_timeout_style(
    config: &mut GrubConfigFile,
    style: TimeoutStyle,
) -> Result<(), BootKeyError> {
    // 再次经过解析校验，确保序列化结果始终合法
    let raw = style.as_str();
    TimeoutStyle::parse(raw)?;
    config.set("GRUB_TIMEOUT_STYLE", raw);
    Ok(())
}

/// 读取默认启动项（键不存在时返回 `None`）
///
/// # Errors
/// 当已配置值非法时返回 [`BootKeyError::InvalidDefaultEntry`]。
pub fn get_default_entry(config: &GrubConfigFile) -> Result<Option<DefaultEntry>, BootKeyError> {
    match config.get("GRUB_DEFAULT") {
        Some(raw) => DefaultEntry::parse(raw).map(Some),
        None => Ok(None),
    }
}

/// 写入默认启动项（保留原有引号与行尾注释）
///
/// # Errors
/// 当取值非法时返回 [`BootKeyError::InvalidDefaultEntry`]。
pub fn set_default_entry(
    config: &mut GrubConfigFile,
    entry: &DefaultEntry,
) -> Result<(), BootKeyError> {
    let value = entry.as_config_value();
    // 回环校验，防止未来扩展时遗漏非法形态
    DefaultEntry::parse(&value)?;
    config.set("GRUB_DEFAULT", &value);
    Ok(())
}

/// 由菜单逻辑序号构造默认项
pub fn default_entry_from_index(index: u32) -> DefaultEntry {
    DefaultEntry::Index(index)
}

/// 由标题或层级路径构造默认项
///
/// # Errors
/// 当标题为空或包含非法字符时返回 [`BootKeyError::InvalidDefaultEntry`]。
pub fn default_entry_from_title(title: &str) -> Result<DefaultEntry, BootKeyError> {
    DefaultEntry::parse(title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_grub_config;

    #[test]
    fn test_timeout_style_parse_and_format() {
        assert_eq!(TimeoutStyle::parse("menu").unwrap(), TimeoutStyle::Menu);
        assert_eq!(TimeoutStyle::parse("hidden").unwrap(), TimeoutStyle::Hidden);
        assert_eq!(
            TimeoutStyle::parse("countdown").unwrap(),
            TimeoutStyle::Countdown
        );
        assert!(TimeoutStyle::parse("MENU ").is_err());
        assert_eq!(TimeoutStyle::Hidden.as_str(), "hidden");
    }

    #[test]
    fn test_timeout_seconds_range() {
        assert_eq!(
            TimeoutSeconds::try_new(-1).unwrap(),
            TimeoutSeconds::INFINITE
        );
        assert_eq!(TimeoutSeconds::try_new(0).unwrap(), TimeoutSeconds(0));
        assert_eq!(
            TimeoutSeconds::try_new(TimeoutSeconds::MAX_FINITE).unwrap(),
            TimeoutSeconds(TimeoutSeconds::MAX_FINITE)
        );
        assert!(TimeoutSeconds::try_new(-2).is_err());
        assert!(TimeoutSeconds::try_new(TimeoutSeconds::MAX_FINITE + 1).is_err());
        assert!(TimeoutSeconds::parse("abc").is_err());
    }

    #[test]
    fn test_default_entry_parse_forms() {
        assert_eq!(DefaultEntry::parse("0").unwrap(), DefaultEntry::Index(0));
        assert_eq!(DefaultEntry::parse("12").unwrap(), DefaultEntry::Index(12));
        assert_eq!(DefaultEntry::parse("saved").unwrap(), DefaultEntry::Saved);
        assert_eq!(
            DefaultEntry::parse("Advanced>Ubuntu").unwrap(),
            DefaultEntry::Title("Advanced>Ubuntu".to_string())
        );
        assert!(DefaultEntry::parse("   ").is_err());
        assert!(DefaultEntry::parse("bad\nline").is_err());
    }

    #[test]
    fn test_set_timeout_preserves_quotes_and_comments() {
        let mut config = parse_grub_config("# 头注释\nGRUB_TIMEOUT='5' # 等待\nGRUB_DEFAULT=0\n");
        set_timeout(&mut config, TimeoutSeconds(10)).unwrap();
        let text = config.serialize();
        assert!(text.contains("GRUB_TIMEOUT='10' # 等待"));
        assert!(text.contains("# 头注释"));
        assert!(text.contains("GRUB_DEFAULT=0"));
    }

    #[test]
    fn test_set_timeout_style_preserves_format() {
        let mut config = parse_grub_config("GRUB_TIMEOUT=3\n# 尾部说明\n");
        set_timeout_style(&mut config, TimeoutStyle::Hidden).unwrap();
        assert_eq!(get_timeout_style(&config).unwrap(), TimeoutStyle::Hidden);
        let text = config.serialize();
        // 新键追加时统一双引号，与 GrubConfigFile::set 既有契约一致
        assert!(text.contains("GRUB_TIMEOUT_STYLE=\"hidden\""));
        assert!(text.contains("# 尾部说明"));
        // 新增键位于末尾，不打乱原键序
        let timeout_pos = text.find("GRUB_TIMEOUT=").unwrap();
        let style_pos = text.find("GRUB_TIMEOUT_STYLE=").unwrap();
        assert!(timeout_pos < style_pos);
    }

    #[test]
    fn test_set_default_entry_semantic_index() {
        let mut config = parse_grub_config("GRUB_DEFAULT=0\n");
        set_default_entry(&mut config, &DefaultEntry::Index(3)).unwrap();
        assert_eq!(config.get("GRUB_DEFAULT"), Some("3"));

        set_default_entry(&mut config, &DefaultEntry::Saved).unwrap();
        assert_eq!(config.get("GRUB_DEFAULT"), Some("saved"));

        set_default_entry(&mut config, &default_entry_from_title("Win > EFI").unwrap()).unwrap();
        assert_eq!(config.get("GRUB_DEFAULT"), Some("Win > EFI"));
        // 含空格标题应自动升级为双引号
        let text = config.serialize();
        assert!(text.contains("GRUB_DEFAULT=\"Win > EFI\""));
    }

    #[test]
    fn test_get_timeout_default_when_missing() {
        let config = parse_grub_config("GRUB_DEFAULT=0\n");
        assert_eq!(get_timeout(&config).unwrap(), TimeoutSeconds(5));
        assert_eq!(get_timeout_style(&config).unwrap(), TimeoutStyle::Menu);
        assert_eq!(
            get_default_entry(&config).unwrap(),
            Some(DefaultEntry::Index(0))
        );
    }

    #[test]
    fn test_invalid_existing_values_rejected() {
        let config =
            parse_grub_config("GRUB_TIMEOUT=abc\nGRUB_TIMEOUT_STYLE=fancy\nGRUB_DEFAULT=\n");
        assert!(get_timeout(&config).is_err());
        assert!(get_timeout_style(&config).is_err());
        assert!(get_default_entry(&config).is_err());
    }
}
