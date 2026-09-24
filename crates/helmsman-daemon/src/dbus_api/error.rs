use crate::service::DaemonError;
use zbus::DBusError;

/// Helmsman D-Bus 服务领域错误枚举
///
/// # 设计原理
/// - **实现初衷**：在 D-Bus 协议层向客户端返回具备明确领域命名空间与中文描述的强类型错误，
///   使 UI 能按错误变体分流提示（鉴权弹窗 / 锁冲突重试 / 回滚建议），而不是只展示一坨字符串。
/// - **核心优势**：通过 zbus DBusError 宏自动映射为 D-Bus 标准错误报文；
///   [`From<DaemonError>`] 统一将特权层领域错误升格为总线契约，避免各调用点手写字符串丢失语义。
/// - **代价与局限**：错误变体需与 [`DaemonError`] 同步演进；新增业务错误时必须补映射。
#[derive(DBusError, Debug)]
#[zbus(prefix = "org.freedesktop.Helmsman.Error")]
pub enum HelmsmanDbusError {
    /// 底层 ZBus 通信错误
    #[zbus(error)]
    ZBus(zbus::Error),
    /// 未通过 PolicyKit 权限核验
    NotAuthorized(String),
    /// 传入参数非法
    InvalidArgs(String),
    /// 业务执行失败（未归类的兜底错误）
    Failed(String),
    /// 包管理器互斥锁冲突
    PackageManagerLocked(String),
    /// 配置文件读写或不存在
    ConfigError(String),
    /// 快照创建/列表/裁剪失败
    SnapshotFailed(String),
    /// 未找到指定快照
    SnapshotNotFound(String),
    /// 快照还原失败
    SnapshotRestoreFailed(String),
    /// 引导生成命令失败（含自动回滚结果说明）
    CommandFailed(String),
    /// 安全策略拦截
    SecurityBlocked(String),
    /// 磁盘空间不足
    DiskSpaceInsufficient(String),
    /// 自定义引导项校验失败
    CustomEntryInvalid(String),
    /// 主题安装/卸载失败
    ThemeFailed(String),
}

impl From<DaemonError> for HelmsmanDbusError {
    /// 将特权层领域错误映射为 D-Bus 错误变体，完整保留中文可读描述
    fn from(err: DaemonError) -> Self {
        match err {
            DaemonError::ConfigReadFailed { path, reason } => {
                HelmsmanDbusError::ConfigError(format!(
                    "读取配置文件失败，路径: {}, 原因: {}",
                    path.display(),
                    reason
                ))
            }
            DaemonError::ConfigNotFound { path } => {
                HelmsmanDbusError::ConfigError(format!("配置文件不存在，路径: {}", path.display()))
            }
            DaemonError::PackageManagerLocked { message } => {
                HelmsmanDbusError::PackageManagerLocked(format!(
                    "包管理器正在占用系统，请稍后再试。{message}"
                ))
            }
            DaemonError::DiskSpaceInsufficient(e) => {
                HelmsmanDbusError::DiskSpaceInsufficient(e.to_string())
            }
            DaemonError::SnapshotFailed { reason } => {
                HelmsmanDbusError::SnapshotFailed(format!("创建配置快照失败，原因: {reason}"))
            }
            DaemonError::AtomicWriteFailed { reason } => {
                HelmsmanDbusError::ConfigError(format!("原子写入新配置失败，原因: {reason}"))
            }
            DaemonError::SecurityCheckFailed(e) => {
                HelmsmanDbusError::SecurityBlocked(e.to_string())
            }
            DaemonError::CommandLaunchFailed { command, reason } => {
                HelmsmanDbusError::CommandFailed(format!(
                    "引导更新命令 '{command}' 执行失败：{reason}"
                ))
            }
            DaemonError::SnapshotListFailed { reason } => {
                HelmsmanDbusError::SnapshotFailed(format!("读取快照列表失败，原因: {reason}"))
            }
            DaemonError::SnapshotNotFound { id } => {
                HelmsmanDbusError::SnapshotNotFound(format!("未找到指定的快照 ID: {id}"))
            }
            DaemonError::SnapshotRestoreFailed { id, reason } => {
                HelmsmanDbusError::SnapshotRestoreFailed(format!(
                    "还原快照 '{id}' 失败，原因: {reason}"
                ))
            }
            DaemonError::ThemeInstallFailed { reason } => {
                HelmsmanDbusError::ThemeFailed(format!("主题操作失败，原因: {reason}"))
            }
            DaemonError::CustomEntryInvalid { reason } => HelmsmanDbusError::CustomEntryInvalid(
                format!("自定义引导项校验失败，原因: {reason}"),
            ),
            DaemonError::InvalidAlias { entry_id, reason } => HelmsmanDbusError::InvalidArgs(
                format!("条目显示别名非法，条目: '{entry_id}'，原因: {reason}"),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_error_maps_to_typed_dbus_error() {
        let locked: HelmsmanDbusError = DaemonError::PackageManagerLocked {
            message: "检测到 /var/lib/dpkg/lock-frontend".to_string(),
        }
        .into();
        assert!(matches!(locked, HelmsmanDbusError::PackageManagerLocked(_)));
        assert!(locked.to_string().contains("包管理器"));

        let not_found: HelmsmanDbusError = DaemonError::SnapshotNotFound {
            id: "snapshot_1".to_string(),
        }
        .into();
        assert!(matches!(not_found, HelmsmanDbusError::SnapshotNotFound(_)));
        assert!(not_found.to_string().contains("snapshot_1"));

        let cmd: HelmsmanDbusError = DaemonError::CommandLaunchFailed {
            command: "/usr/sbin/update-grub".to_string(),
            reason: "已成功自动回滚至初始状态".to_string(),
        }
        .into();
        assert!(matches!(cmd, HelmsmanDbusError::CommandFailed(_)));
        assert!(cmd.to_string().contains("已成功自动回滚"));

        let custom: HelmsmanDbusError = DaemonError::CustomEntryInvalid {
            reason: "条目 ID 'x' 重复".to_string(),
        }
        .into();
        assert!(matches!(custom, HelmsmanDbusError::CustomEntryInvalid(_)));
    }
}
