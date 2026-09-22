use crate::executor::SecurityError;
use grub_transaction_engine::DiskSpaceError;
use std::path::PathBuf;
use thiserror::Error;

/// 特权后台服务领域错误枚举
///
/// # 设计原理
/// - **实现初衷**：禁止底层抛出无类型字符串；错误描述全中文并附带动态现场参数，
///   便于上层精准匹配与排查。
/// - **核心优势**：`thiserror` 派生 `Display`/`Error`，保持错误链完整。
#[derive(Debug, PartialEq, Eq, Error)]
pub enum DaemonError {
    /// 配置文件读取失败
    #[error("读取配置文件失败，路径: {}, 原因: {reason}", path.display())]
    ConfigReadFailed {
        /// 配置文件路径
        path: PathBuf,
        /// 失败原因
        reason: String,
    },
    /// 配置文件不存在
    #[error("配置文件不存在，路径: {}", path.display())]
    ConfigNotFound {
        /// 配置文件路径
        path: PathBuf,
    },
    /// 包管理器互斥锁冲突
    #[error("无法执行引导修改，{message}")]
    PackageManagerLocked {
        /// 锁冲突说明
        message: String,
    },
    /// 关键分区可用磁盘空间不足
    #[error(transparent)]
    DiskSpaceInsufficient(#[from] DiskSpaceError),
    /// 快照创建失败
    #[error("创建配置快照失败，原因: {reason}")]
    SnapshotFailed {
        /// 失败原因
        reason: String,
    },
    /// 原子写入失败
    #[error("原子写入新配置失败，原因: {reason}")]
    AtomicWriteFailed {
        /// 失败原因
        reason: String,
    },
    /// 安全策略检查失败（非白名单程序或参数注入风险）
    #[error(transparent)]
    SecurityCheckFailed(#[from] SecurityError),
    /// 引导生成命令启动失败
    #[error("启动更新命令 '{command}' 失败，原因: {reason}")]
    CommandLaunchFailed {
        /// 命令
        command: String,
        /// 失败原因
        reason: String,
    },
    /// 快照列表读取失败
    #[error("读取快照列表失败，原因: {reason}")]
    SnapshotListFailed {
        /// 失败原因
        reason: String,
    },
    /// 未找到指定快照
    #[error("未找到指定的快照 ID: {id}")]
    SnapshotNotFound {
        /// 快照 ID
        id: String,
    },
    /// 快照还原失败
    #[error("还原快照 '{id}' 失败，原因: {reason}")]
    SnapshotRestoreFailed {
        /// 快照 ID
        id: String,
        /// 失败原因
        reason: String,
    },
    /// 主题安装失败
    #[error("安装主题压缩包失败，原因: {reason}")]
    ThemeInstallFailed {
        /// 失败原因
        reason: String,
    },
    /// 自定义引导项字段未通过安全校验
    #[error("自定义引导项校验失败，原因: {reason}")]
    CustomEntryInvalid {
        /// 校验失败原因
        reason: String,
    },
}
