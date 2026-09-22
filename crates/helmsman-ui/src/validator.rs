use crate::i18n::{Language, TextKey, format_text, get_text};
use grub_config_parser::GrubConfigFile;
use std::fmt;

/// 配置安全风险等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    /// 安全配置，无风险
    Safe,
    /// 提示或轻度警告（如内核参数为空，需用户确认）
    Warning,
    /// 致命高危配置（如超时设为 0 且静默，禁止或强阻断提交）
    DangerBlocked,
}

impl RiskLevel {
    /// 获取指定语言的风险等级标签
    pub fn label(&self, lang: Language) -> &'static str {
        match self {
            RiskLevel::Safe => get_text(lang, TextKey::RiskSafe),
            RiskLevel::Warning => get_text(lang, TextKey::RiskWarning),
            RiskLevel::DangerBlocked => get_text(lang, TextKey::RiskDanger),
        }
    }
}

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 日志与调试输出默认中文，与工程日志规范一致
        write!(f, "{}", self.label(Language::ZhCn))
    }
}

/// 配置安全校验问题详情
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    /// 风险等级
    pub level: RiskLevel,
    /// 规则简要标题
    pub title: String,
    /// 详细风险说明与后果预警
    pub message: String,
    /// 用户在了解风险后是否仍可强制继续提交
    pub can_proceed: bool,
}

/// 配置安全规则校验器
pub struct SafetyValidator;

impl SafetyValidator {
    /// 校验待提交的 GRUB 配置文件，输出指定语言的风险问题列表
    ///
    /// # 设计原理
    /// - **实现初衷**：杜绝用户误操作将系统引导修改至不可恢复状态（开机变砖）。
    /// - **核心优势**：纯内存规则引擎，在用户发起特权写入请求前第一步拦截，保障安全性。
    pub fn validate(config: &GrubConfigFile, lang: Language) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // 规则 1：检查倒计时与静默模式结合（致命阻断）
        let timeout_str = config.get("GRUB_TIMEOUT").unwrap_or("5");
        let timeout_val = timeout_str.parse::<i32>().unwrap_or(5);
        let timeout_style = config.get("GRUB_TIMEOUT_STYLE").unwrap_or("");

        if timeout_val == 0 && (timeout_style == "hidden" || timeout_style.is_empty()) {
            issues.push(ValidationIssue {
                level: RiskLevel::DangerBlocked,
                title: get_text(lang, TextKey::ValidTimeoutZeroTitle).to_string(),
                message: get_text(lang, TextKey::ValidTimeoutZeroMessage).to_string(),
                can_proceed: false,
            });
        }

        // 规则 2：检查默认启动项是否为空
        if let Some(default_entry) = config.get("GRUB_DEFAULT")
            && default_entry.trim().is_empty()
        {
            issues.push(ValidationIssue {
                level: RiskLevel::Warning,
                title: get_text(lang, TextKey::ValidEmptyDefaultTitle).to_string(),
                message: get_text(lang, TextKey::ValidEmptyDefaultMessage).to_string(),
                can_proceed: true,
            });
        }

        // 规则 3：检查内核引导参数被意外清空
        let cmdline_default = config.get("GRUB_CMDLINE_LINUX_DEFAULT");
        let cmdline_linux = config.get("GRUB_CMDLINE_LINUX");

        let is_empty_default = cmdline_default.map(|s| s.trim().is_empty()).unwrap_or(true);
        let is_empty_linux = cmdline_linux.map(|s| s.trim().is_empty()).unwrap_or(true);

        if is_empty_default && is_empty_linux {
            issues.push(ValidationIssue {
                level: RiskLevel::Warning,
                title: get_text(lang, TextKey::ValidEmptyCmdlineTitle).to_string(),
                message: get_text(lang, TextKey::ValidEmptyCmdlineMessage).to_string(),
                can_proceed: true,
            });
        }

        // 规则 4：检查主题描述文件路径格式
        if let Some(theme_path) = config.get("GRUB_THEME") {
            let trimmed = theme_path.trim();
            if !trimmed.is_empty() && !trimmed.ends_with("theme.txt") {
                issues.push(ValidationIssue {
                    level: RiskLevel::Warning,
                    title: get_text(lang, TextKey::ValidThemePathTitle).to_string(),
                    message: format_text(
                        lang,
                        TextKey::ValidThemePathMessage,
                        &[("path", trimmed)],
                    ),
                    can_proceed: true,
                });
            }
        }

        // 规则 5：检查背景壁纸图形格式
        if let Some(bg_path) = config.get("GRUB_BACKGROUND") {
            let trimmed = bg_path.trim().to_lowercase();
            let valid_extensions = [".png", ".jpg", ".jpeg", ".tga"];
            let has_valid_ext = valid_extensions.iter().any(|ext| trimmed.ends_with(ext));
            if !trimmed.is_empty() && !has_valid_ext {
                issues.push(ValidationIssue {
                    level: RiskLevel::Warning,
                    title: get_text(lang, TextKey::ValidBgFormatTitle).to_string(),
                    message: format_text(
                        lang,
                        TextKey::ValidBgFormatMessage,
                        &[("path", bg_path.trim())],
                    ),
                    can_proceed: true,
                });
            }
        }

        issues
    }

    /// 评估问题列表中最高的风险等级
    pub fn max_risk_level(issues: &[ValidationIssue]) -> RiskLevel {
        issues
            .iter()
            .map(|i| i.level)
            .max()
            .unwrap_or(RiskLevel::Safe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grub_config_parser::parse_grub_config;

    #[test]
    fn test_validate_safe_config() {
        let content =
            "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\"\n";
        let config = parse_grub_config(content);
        let issues = SafetyValidator::validate(&config, Language::ZhCn);

        assert!(issues.is_empty());
        assert_eq!(SafetyValidator::max_risk_level(&issues), RiskLevel::Safe);
    }

    #[test]
    fn test_validate_danger_zero_timeout_hidden() {
        let content = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=0\nGRUB_TIMEOUT_STYLE=hidden\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet\"\n";
        let config = parse_grub_config(content);
        let issues = SafetyValidator::validate(&config, Language::ZhCn);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].level, RiskLevel::DangerBlocked);
        assert!(!issues[0].can_proceed);
        assert_eq!(issues[0].title, "倒计时为 0 且处于静默模式");
        assert_eq!(
            SafetyValidator::max_risk_level(&issues),
            RiskLevel::DangerBlocked
        );

        let en = SafetyValidator::validate(&config, Language::EnUs);
        assert_eq!(en[0].title, "Timeout is 0 with silent mode");
    }

    #[test]
    fn test_validate_empty_kernel_params_warning() {
        let content = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT=\"\"\n";
        let config = parse_grub_config(content);
        let issues = SafetyValidator::validate(&config, Language::ZhCn);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].level, RiskLevel::Warning);
        assert!(issues[0].can_proceed);
        assert_eq!(issues[0].title, "内核引导参数为空");
    }
}
