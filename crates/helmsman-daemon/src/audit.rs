use std::env;
use std::time::Duration;
use tracing::info;

/// 审计事件操作类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditAction {
    /// 提交配置修改事务
    ApplyChanges,
    /// 快速切换默认启动项
    SetDefaultFast,
    /// 还原历史快照
    RollbackSnapshot,
    /// 安装主题压缩包
    InstallTheme,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditAction::ApplyChanges => write!(f, "提交配置修改事务"),
            AuditAction::SetDefaultFast => write!(f, "快速切换默认启动项"),
            AuditAction::RollbackSnapshot => write!(f, "还原历史快照"),
            AuditAction::InstallTheme => write!(f, "安装主题压缩包"),
        }
    }
}

/// 特权变更结构化审计事件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// 触发操作的调用方真实 UID
    pub caller_uid: u32,
    /// 审计操作类型
    pub action: AuditAction,
    /// 操作目标描述或原因
    pub reason: String,
    /// 关联快照 ID（若有）
    pub snapshot_id: Option<String>,
    /// 操作是否成功
    pub success: bool,
    /// 操作执行耗时
    pub duration: Duration,
    /// 变更行数摘要（如 "+2 / -1"）
    pub diff_summary: Option<String>,
}

/// 记录特权审计事件至结构化日志与 journald
///
/// # 设计原理
/// - **实现初衷**：满足系统安全与等保合规要求，记录特权变更的完整审计轨迹。
/// - **核心优势**：通过 tracing 结构化字段注入，配合 systemd journald 实现低开销、可检索的日志持久化。
pub fn record_audit_event(event: &AuditEvent) {
    let snapshot_id_str = event.snapshot_id.as_deref().unwrap_or("none");
    let diff_str = event.diff_summary.as_deref().unwrap_or("none");

    info!(
        target: "helmsman::audit",
        caller_uid = event.caller_uid,
        action = %event.action,
        reason = %event.reason,
        snapshot_id = %snapshot_id_str,
        success = event.success,
        duration_ms = event.duration.as_millis() as u64,
        diff = %diff_str,
        "特权审计：UID {} 执行 [{}] - 状态: {} (耗时 {} ms)",
        event.caller_uid,
        event.action,
        if event.success { "成功" } else { "失败" },
        event.duration.as_millis()
    );
}

/// 解析调用方的真实用户 UID
///
/// 优先从 PolicyKit (`PKEXEC_UID`)、Sudo (`SUDO_UID`) 环境变量中提取，
/// 若均不存在则默认回退到 0（root 守护进程特权上下文）。
pub fn resolve_caller_uid() -> u32 {
    resolve_caller_uid_from_lookup(|k| env::var(k).ok())
}

/// 内部 UID 解析实现（支持安全无 unsafe 依赖注入测试）
pub(crate) fn resolve_caller_uid_from_lookup<F>(lookup: F) -> u32
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(uid_str) = lookup("PKEXEC_UID")
        && let Ok(uid) = uid_str.parse::<u32>()
    {
        return uid;
    }

    if let Some(uid_str) = lookup("SUDO_UID")
        && let Ok(uid) = uid_str.parse::<u32>()
    {
        return uid;
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_creation_and_format() {
        let event = AuditEvent {
            caller_uid: 1000,
            action: AuditAction::ApplyChanges,
            reason: "调整倒计时为 10 秒".to_string(),
            snapshot_id: Some("snapshot_12345".to_string()),
            success: true,
            duration: Duration::from_millis(150),
            diff_summary: Some("+1 / -1".to_string()),
        };

        // 验证结构体字段完整性
        assert_eq!(event.caller_uid, 1000);
        assert_eq!(event.action, AuditAction::ApplyChanges);
        assert!(event.success);

        // 验证日志记录不会 panic
        record_audit_event(&event);
    }

    #[test]
    fn test_resolve_caller_uid_env_mock() {
        // 测试通过 PKEXEC_UID 注入
        let uid1 = resolve_caller_uid_from_lookup(|k| {
            if k == "PKEXEC_UID" {
                Some("1002".to_string())
            } else {
                None
            }
        });
        assert_eq!(uid1, 1002);

        // 测试通过 SUDO_UID 注入
        let uid2 = resolve_caller_uid_from_lookup(|k| {
            if k == "SUDO_UID" {
                Some("1001".to_string())
            } else {
                None
            }
        });
        assert_eq!(uid2, 1001);

        // 测试无环境变量时回退到 0
        let uid_default = resolve_caller_uid_from_lookup(|_| None);
        assert_eq!(uid_default, 0);
    }
}
