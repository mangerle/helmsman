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

impl fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RiskLevel::Safe => write!(f, "安全"),
            RiskLevel::Warning => write!(f, "警告"),
            RiskLevel::DangerBlocked => write!(f, "致命危险"),
        }
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
    /// 校验待提交的 GRUB 配置文件，输出发现的所有风险问题
    ///
    /// # 设计原理
    /// - **实现初衷**：杜绝用户误操作将系统引导修改至不可恢复状态（开机变砖）。
    /// - **核心优势**：纯内存规则引擎，在用户发起特权写入请求前第一步拦截，保障安全性。
    pub fn validate(config: &GrubConfigFile) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // 规则 1：检查倒计时与静默模式结合（致命阻断）
        let timeout_str = config.get("GRUB_TIMEOUT").unwrap_or("5");
        let timeout_val = timeout_str.parse::<i32>().unwrap_or(5);
        let timeout_style = config.get("GRUB_TIMEOUT_STYLE").unwrap_or("");

        if timeout_val == 0 && (timeout_style == "hidden" || timeout_style.is_empty()) {
            issues.push(ValidationIssue {
                level: RiskLevel::DangerBlocked,
                title: "倒计时为 0 且处于静默模式".to_string(),
                message: "开机等待时间设为 0 且未显示菜单，将导致系统开机时完全无法通过按 Esc 唤出救援菜单。一旦默认内核异常，系统将无法自愈进入恢复模式！".to_string(),
                can_proceed: false,
            });
        }

        // 规则 2：检查默认启动项是否为空
        if let Some(default_entry) = config.get("GRUB_DEFAULT")
            && default_entry.trim().is_empty()
        {
            issues.push(ValidationIssue {
                level: RiskLevel::Warning,
                title: "默认启动项为空".to_string(),
                message: "未指定明确的默认启动项，GRUB 将默认选择菜单第一项。若第一项非正常内核，可能导致启动异常。".to_string(),
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
                title: "内核引导参数为空".to_string(),
                message: "全局内核参数完全为空，可能会丢失关键驱动参数或根分区挂载选项。"
                    .to_string(),
                can_proceed: true,
            });
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
        let issues = SafetyValidator::validate(&config);

        assert!(issues.is_empty());
        assert_eq!(SafetyValidator::max_risk_level(&issues), RiskLevel::Safe);
    }

    #[test]
    fn test_validate_danger_zero_timeout_hidden() {
        let content = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=0\nGRUB_TIMEOUT_STYLE=hidden\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet\"\n";
        let config = parse_grub_config(content);
        let issues = SafetyValidator::validate(&config);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].level, RiskLevel::DangerBlocked);
        assert!(!issues[0].can_proceed);
        assert_eq!(
            SafetyValidator::max_risk_level(&issues),
            RiskLevel::DangerBlocked
        );
    }

    #[test]
    fn test_validate_empty_kernel_params_warning() {
        let content = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT=\"\"\n";
        let config = parse_grub_config(content);
        let issues = SafetyValidator::validate(&config);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].level, RiskLevel::Warning);
        assert!(issues[0].can_proceed);
    }
}
