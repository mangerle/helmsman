use grub_distro_adapter::DistroProfile;
use grub_transaction_engine::{
    DiffReport, SnapshotMeta, atomic_write, create_snapshot, generate_unified_diff, list_snapshots,
    restore_snapshot,
};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info, warn};

/// 特权后台服务领域错误枚举
///
/// # 设计原理
/// - **实现初衷**：根据规范要求，禁止底层抛出无类型字符串。定义具备详尽动态上下文参数的错误枚举，
///   便于上层调用方进行精准匹配、展示中文错误详情及排查定位。
#[derive(Debug, PartialEq, Eq)]
pub enum DaemonError {
    /// 配置文件读取失败
    ConfigReadFailed { path: PathBuf, reason: String },
    /// 配置文件不存在
    ConfigNotFound { path: PathBuf },
    /// 快照创建失败
    SnapshotFailed { reason: String },
    /// 原子写入失败
    AtomicWriteFailed { reason: String },
    /// 引导生成命令启动失败
    CommandLaunchFailed { command: String, reason: String },
    /// 快照列表读取失败
    SnapshotListFailed { reason: String },
    /// 未找到指定快照
    SnapshotNotFound { id: String },
    /// 快照还原失败
    SnapshotRestoreFailed { id: String, reason: String },
}

impl fmt::Display for DaemonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DaemonError::ConfigReadFailed { path, reason } => {
                write!(
                    f,
                    "读取配置文件失败，路径: {}，原因: {}",
                    path.display(),
                    reason
                )
            }
            DaemonError::ConfigNotFound { path } => {
                write!(f, "配置文件不存在，路径: {}", path.display())
            }
            DaemonError::SnapshotFailed { reason } => {
                write!(f, "创建配置快照失败，原因: {}", reason)
            }
            DaemonError::AtomicWriteFailed { reason } => {
                write!(f, "原子写入新配置失败，原因: {}", reason)
            }
            DaemonError::CommandLaunchFailed { command, reason } => {
                write!(f, "启动更新命令 '{}' 失败，原因: {}", command, reason)
            }
            DaemonError::SnapshotListFailed { reason } => {
                write!(f, "读取快照列表失败，原因: {}", reason)
            }
            DaemonError::SnapshotNotFound { id } => {
                write!(f, "未找到指定的快照 ID: {}", id)
            }
            DaemonError::SnapshotRestoreFailed { id, reason } => {
                write!(f, "还原快照 '{}' 失败，原因: {}", id, reason)
            }
        }
    }
}

impl Error for DaemonError {}

/// 事务应用选项配置
#[derive(Debug, Clone, Default)]
pub struct TransactionOptions {
    /// 是否跳过实际引导生成命令的执行（用于单元测试与模拟演练）
    pub skip_command_execution: bool,
}

/// 事务应用结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionResult {
    /// 事务是否成功提交
    pub success: bool,
    /// 生成的快照 ID
    pub snapshot_id: String,
    /// 引导生成命令的控制台输出日志
    pub log_output: String,
    /// 失败时的错误原因
    pub error_message: Option<String>,
}

/// GRUB 特权业务协调服务
pub struct GrubService {
    /// /etc/default/grub 路径
    pub default_config_path: PathBuf,
    /// 备份根目录
    pub backup_dir: PathBuf,
    /// 跨发行版档案
    pub distro_profile: DistroProfile,
}

impl GrubService {
    /// 创建标准系统服务实例
    pub fn new_system_default() -> Self {
        let distro_profile = DistroProfile::detect_current_system();
        Self {
            default_config_path: PathBuf::from("/etc/default/grub"),
            backup_dir: PathBuf::from("/var/backups/grub-manager"),
            distro_profile,
        }
    }

    /// 创建自定义路径实例（用于单元测试与集成测试）
    pub fn new_with_paths(
        default_config_path: PathBuf,
        backup_dir: PathBuf,
        distro_profile: DistroProfile,
    ) -> Self {
        Self {
            default_config_path,
            backup_dir,
            distro_profile,
        }
    }

    /// 预览配置差异 (Diff)
    ///
    /// # Errors
    /// 当目标配置文件不存在或无法读取时返回 `DaemonError::ConfigReadFailed`。
    pub fn preview_diff(&self, new_config: &str) -> Result<DiffReport, DaemonError> {
        let original = fs::read_to_string(&self.default_config_path).map_err(|e| {
            DaemonError::ConfigReadFailed {
                path: self.default_config_path.clone(),
                reason: e.to_string(),
            }
        })?;
        Ok(generate_unified_diff(&original, new_config))
    }

    /// 执行配置提交事务（包含快照、原子替换、引导编译与失败自动回滚）
    ///
    /// # Errors
    /// 当文件不存在、快照创建失败或写入失败时返回对应的 `DaemonError`。
    pub fn apply_changes(
        &self,
        new_config: &str,
        reason: &str,
        options: &TransactionOptions,
    ) -> Result<TransactionResult, DaemonError> {
        if !self.default_config_path.exists() {
            return Err(DaemonError::ConfigNotFound {
                path: self.default_config_path.clone(),
            });
        }

        debug!("开始准备配置变更事务，原因: {}", reason);

        let snapshot = create_snapshot(&self.default_config_path, &self.backup_dir, reason)
            .map_err(|e| DaemonError::SnapshotFailed {
                reason: e.to_string(),
            })?;

        if let Err(e) = atomic_write(&self.default_config_path, new_config) {
            return Err(DaemonError::AtomicWriteFailed {
                reason: e.to_string(),
            });
        }
        debug!("新配置原子替换成功，待触发引导更新");

        if options.skip_command_execution {
            info!("跳过引导命令执行（模拟测试模式），快照 ID: {}", snapshot.id);
            return Ok(TransactionResult {
                success: true,
                snapshot_id: snapshot.id,
                log_output: "跳过引导命令执行（模拟测试模式）".to_string(),
                error_message: None,
            });
        }

        self.execute_update_with_rollback(&snapshot)
    }

    /// 执行引导更新命令并在失败时自动触发快照回滚
    fn execute_update_with_rollback(
        &self,
        snapshot: &SnapshotMeta,
    ) -> Result<TransactionResult, DaemonError> {
        let mut cmd = Command::new(&self.distro_profile.update_command);
        cmd.args(&self.distro_profile.command_args);

        match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let combined_log = format!("{}\n{}", stdout, stderr);

                if output.status.success() {
                    info!("引导更新命令执行成功，快照 ID: {}", snapshot.id);
                    Ok(TransactionResult {
                        success: true,
                        snapshot_id: snapshot.id.clone(),
                        log_output: combined_log,
                        error_message: None,
                    })
                } else {
                    warn!(
                        "引导生成命令退出码非零 ({:?})，触发自动回滚，快照 ID: {}",
                        output.status.code(),
                        snapshot.id
                    );
                    let rollback_err = restore_snapshot(snapshot)
                        .err()
                        .map(|e| format!("且自动回滚失败: {}", e))
                        .unwrap_or_else(|| "已成功自动回滚至初始状态".to_string());

                    Ok(TransactionResult {
                        success: false,
                        snapshot_id: snapshot.id.clone(),
                        log_output: combined_log,
                        error_message: Some(format!(
                            "引导生成命令退出码非零 ({:?})，{}",
                            output.status.code(),
                            rollback_err
                        )),
                    })
                }
            }
            Err(e) => {
                warn!("启动更新命令失败，触发自动回滚，原因: {}", e);
                let _ = restore_snapshot(snapshot);
                Err(DaemonError::CommandLaunchFailed {
                    command: self.distro_profile.update_command.clone(),
                    reason: format!("{}（已自动回滚）", e),
                })
            }
        }
    }

    /// 一键回滚至指定快照
    ///
    /// # Errors
    /// 当快照列表无法读取、未找到 ID 或还原失败时返回对应的 `DaemonError`。
    pub fn rollback_to_snapshot(&self, snapshot_id: &str) -> Result<(), DaemonError> {
        let snapshots =
            list_snapshots(&self.backup_dir).map_err(|e| DaemonError::SnapshotListFailed {
                reason: e.to_string(),
            })?;

        let target_snapshot = snapshots
            .into_iter()
            .find(|s| s.id == snapshot_id)
            .ok_or_else(|| DaemonError::SnapshotNotFound {
                id: snapshot_id.to_string(),
            })?;

        restore_snapshot(&target_snapshot).map_err(|e| DaemonError::SnapshotRestoreFailed {
            id: snapshot_id.to_string(),
            reason: e.to_string(),
        })?;

        info!("成功还原历史快照: {}", snapshot_id);
        Ok(())
    }

    /// 列出所有可用快照
    ///
    /// # Errors
    /// 当备份目录无法访问时返回 `DaemonError::SnapshotListFailed`。
    pub fn get_available_snapshots(&self) -> Result<Vec<SnapshotMeta>, DaemonError> {
        list_snapshots(&self.backup_dir).map_err(|e| DaemonError::SnapshotListFailed {
            reason: e.to_string(),
        })
    }
}
