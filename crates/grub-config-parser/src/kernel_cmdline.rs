//! 内核启动命令行参数（`GRUB_CMDLINE_LINUX` / `GRUB_CMDLINE_LINUX_DEFAULT`）解析与规范化。
//!
//! # 设计原理
//! - **实现初衷**：内核参数最终注入 GRUB 脚本与内核命令行，空白重复项与危险元字符
//!   既难排查又可能破坏生成脚本。将参数收敛为有序令牌列表，在写回前统一去重与校验。
//! - **核心优势**：保持首次出现顺序（用户可读、diff 稳定）；常见可视化开关
//!   （quiet / splash / nomodeset 等）提供独立启停 API，避免字符串拼接出错。
//! - **代价与局限**：按空白分词，不解析带空格的复杂引号参数（内核参数本身极少需要）；
//!   校验采用保守白名单，拒绝非常规元字符。

use crate::ast::GrubConfigFile;
use thiserror::Error;

/// 常见可视化/诊断开关常量
pub mod well_known_flags {
    /// 静默启动，隐藏大部分内核日志
    pub const QUIET: &str = "quiet";
    /// 显示启动飞溅画面（通常配合发行版 plymouth）
    pub const SPLASH: &str = "splash";
    /// 禁用内核模式设置，显卡异常时的通用回退
    pub const NOMODESET: &str = "nomodeset";
    /// 单用户/救援模式
    pub const SINGLE: &str = "single";
    /// 进入救援 shell
    pub const RESCUE: &str = "rescue";
    /// 打印详细内核日志
    pub const DEBUG: &str = "debug";
}

/// 内核参数解析或校验错误
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CmdlineError {
    /// 单个参数令牌非法
    #[error("内核参数 '{token}' 非法，原因: {reason}")]
    InvalidToken {
        /// 被拒绝的令牌
        token: String,
        /// 拒绝原因
        reason: String,
    },
    /// 整体参数串超长
    #[error("内核参数串过长（{length} 字符），上限 {limit} 字符")]
    TooLong {
        /// 实际长度
        length: usize,
        /// 允许上限
        limit: usize,
    },
}

/// 有序、去重、已校验的内核命令行参数列表
///
/// # 设计原理
/// - **实现初衷**：以令牌列表作为唯一真源，序列化时再拼接，避免反复对字符串做脆弱的空格处理。
/// - **核心优势**：插入时自动去重并保持首次出现顺序，`enable_flag` / `disable_flag`
///   可安全启停常见开关。
/// - **代价与局限**：不保留原始串中的多余空白排版（规范化会折叠为空格分隔），
///   但 `GrubConfigFile::set` 仍会保留该键原有的引号与行尾注释。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KernelCmdline {
    /// 有序参数令牌（含 `key=value` 形态）
    tokens: Vec<String>,
}

impl KernelCmdline {
    /// 参数串最大长度（防止异常超长注入）
    pub const MAX_LENGTH: usize = 2048;

    /// 创建空参数列表
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    /// 以已知容量预分配
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            tokens: Vec::with_capacity(capacity),
        }
    }

    /// 从原始参数串解析（自动去空白、去重、校验）
    ///
    /// # Errors
    /// 当原始串含控制字符、存在非法令牌或总长超限时返回 [`CmdlineError`]。
    pub fn parse(raw: &str) -> Result<Self, CmdlineError> {
        if raw.len() > Self::MAX_LENGTH {
            return Err(CmdlineError::TooLong {
                length: raw.len(),
                limit: Self::MAX_LENGTH,
            });
        }
        // 分词前拒绝控制字符：换行/制表符若被当作分隔符静默吞掉，会掩盖注入意图
        if raw.chars().any(|c| c.is_control() && c != ' ') {
            return Err(CmdlineError::InvalidToken {
                token: raw.chars().take(32).collect(),
                reason: "参数串禁止包含换行、制表符等控制字符".to_string(),
            });
        }

        let mut cmdline = Self::with_capacity(raw.split_whitespace().count());
        for token in raw.split_whitespace() {
            cmdline.push_unique(token)?;
        }
        Ok(cmdline)
    }

    /// 获取有序令牌切片
    pub fn tokens(&self) -> &[String] {
        &self.tokens
    }

    /// 是否包含指定开关（精确匹配令牌，不含 `key=` 前缀比较）
    pub fn contains_flag(&self, flag: &str) -> bool {
        self.tokens.iter().any(|t| t == flag)
    }

    /// 追加令牌（已存在则忽略），保持首次出现顺序
    ///
    /// # Errors
    /// 当令牌非法时返回 [`CmdlineError::InvalidToken`]。
    pub fn push_unique(&mut self, token: &str) -> Result<bool, CmdlineError> {
        validate_token(token)?;
        if self.tokens.iter().any(|t| t == token) {
            return Ok(false);
        }
        self.tokens.push(token.to_string());
        Ok(true)
    }

    /// 移除首个匹配令牌，返回是否发生移除
    pub fn remove_flag(&mut self, flag: &str) -> bool {
        if let Some(pos) = self.tokens.iter().position(|t| t == flag) {
            self.tokens.remove(pos);
            return true;
        }
        false
    }

    /// 启用开关（幂等）
    ///
    /// # Errors
    /// 当开关名非法时返回 [`CmdlineError::InvalidToken`]。
    pub fn enable_flag(&mut self, flag: &str) -> Result<(), CmdlineError> {
        self.push_unique(flag)?;
        Ok(())
    }

    /// 停用开关（幂等）
    pub fn disable_flag(&mut self, flag: &str) {
        self.remove_flag(flag);
    }

    /// 序列化为空格分隔参数串
    pub fn as_str(&self) -> String {
        self.tokens.join(" ")
    }
}

/// 校验单个内核参数令牌
///
/// # Errors
/// 当令牌为空、包含控制字符或 shell/GRUB 元字符时返回 [`CmdlineError::InvalidToken`]。
pub fn validate_token(token: &str) -> Result<(), CmdlineError> {
    if token.is_empty() {
        return Err(CmdlineError::InvalidToken {
            token: token.to_string(),
            reason: "参数令牌不能为空".to_string(),
        });
    }
    if token.len() > 256 {
        return Err(CmdlineError::InvalidToken {
            token: token.to_string(),
            reason: "单个参数令牌不得超过 256 字符".to_string(),
        });
    }

    let has_forbidden = token.chars().any(|c| {
        matches!(
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
                | '\t'
        ) || c.is_control()
    });
    if has_forbidden {
        return Err(CmdlineError::InvalidToken {
            token: token.to_string(),
            reason: "禁止包含换行、引号或 shell 元字符".to_string(),
        });
    }
    Ok(())
}

/// 读取 `GRUB_CMDLINE_LINUX` 为类型化参数列表
///
/// # Errors
/// 当已配置值非法时返回 [`CmdlineError`]。
pub fn get_cmdline_linux(config: &GrubConfigFile) -> Result<KernelCmdline, CmdlineError> {
    let raw = config.get("GRUB_CMDLINE_LINUX").unwrap_or("");
    KernelCmdline::parse(raw)
}

/// 写入 `GRUB_CMDLINE_LINUX`（保留原有引号与行尾注释）
///
/// # Errors
/// 当参数列表序列化后仍含非法内容时返回 [`CmdlineError`]（防御性回环校验）。
pub fn set_cmdline_linux(
    config: &mut GrubConfigFile,
    cmdline: &KernelCmdline,
) -> Result<(), CmdlineError> {
    let value = cmdline.as_str();
    KernelCmdline::parse(&value)?;
    config.set("GRUB_CMDLINE_LINUX", &value);
    Ok(())
}

/// 读取 `GRUB_CMDLINE_LINUX_DEFAULT` 为类型化参数列表
///
/// # Errors
/// 当已配置值非法时返回 [`CmdlineError`]。
pub fn get_cmdline_default(config: &GrubConfigFile) -> Result<KernelCmdline, CmdlineError> {
    let raw = config.get("GRUB_CMDLINE_LINUX_DEFAULT").unwrap_or("");
    KernelCmdline::parse(raw)
}

/// 写入 `GRUB_CMDLINE_LINUX_DEFAULT`（保留原有引号与行尾注释）
///
/// # Errors
/// 当参数列表序列化后仍含非法内容时返回 [`CmdlineError`]（防御性回环校验）。
pub fn set_cmdline_default(
    config: &mut GrubConfigFile,
    cmdline: &KernelCmdline,
) -> Result<(), CmdlineError> {
    let value = cmdline.as_str();
    KernelCmdline::parse(&value)?;
    config.set("GRUB_CMDLINE_LINUX_DEFAULT", &value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_grub_config;

    #[test]
    fn test_parse_dedup_preserves_first_order() {
        let cmdline = KernelCmdline::parse("quiet splash quiet nomodeset splash").unwrap();
        assert_eq!(
            cmdline.tokens(),
            &[
                "quiet".to_string(),
                "splash".to_string(),
                "nomodeset".to_string()
            ]
        );
        assert_eq!(cmdline.as_str(), "quiet splash nomodeset");
    }

    #[test]
    fn test_parse_key_value_token() {
        let cmdline = KernelCmdline::parse("root=UUID=abc-123 ro quiet").unwrap();
        assert!(cmdline.contains_flag("root=UUID=abc-123"));
        assert!(cmdline.contains_flag("ro"));
        assert!(cmdline.contains_flag("quiet"));
    }

    #[test]
    fn test_reject_dangerous_tokens() {
        assert!(KernelCmdline::parse("quiet; rm -rf /").is_err());
        assert!(KernelCmdline::parse("quiet $(evil)").is_err());
        assert!(KernelCmdline::parse("quiet\nbad").is_err());
    }

    #[test]
    fn test_enable_disable_well_known_flags() {
        let mut cmdline = KernelCmdline::parse("quiet splash").unwrap();
        cmdline.enable_flag(well_known_flags::NOMODESET).unwrap();
        assert!(cmdline.contains_flag("nomodeset"));
        // 幂等
        cmdline.enable_flag(well_known_flags::NOMODESET).unwrap();
        assert_eq!(
            cmdline.tokens(),
            &[
                "quiet".to_string(),
                "splash".to_string(),
                "nomodeset".to_string()
            ]
        );

        cmdline.disable_flag(well_known_flags::QUIET);
        assert!(!cmdline.contains_flag("quiet"));
        assert_eq!(cmdline.as_str(), "splash nomodeset");
    }

    #[test]
    fn test_set_cmdline_preserves_quotes_and_comments() {
        let mut config =
            parse_grub_config("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\" # 默认参数\n");
        let mut cmdline = get_cmdline_default(&config).unwrap();
        cmdline.enable_flag("nomodeset").unwrap();
        set_cmdline_default(&mut config, &cmdline).unwrap();

        let text = config.serialize();
        assert!(text.contains("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash nomodeset\" # 默认参数"));
    }

    #[test]
    fn test_empty_cmdline_allowed() {
        let mut config = parse_grub_config("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet\"\n");
        set_cmdline_default(&mut config, &KernelCmdline::new()).unwrap();
        assert_eq!(config.get("GRUB_CMDLINE_LINUX_DEFAULT"), Some(""));
        assert!(get_cmdline_default(&config).unwrap().tokens().is_empty());
    }
}
