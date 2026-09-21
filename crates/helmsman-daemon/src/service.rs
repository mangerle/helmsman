use crate::audit::{AuditAction, AuditEvent, record_audit_event, resolve_caller_uid};
use crate::executor::{SafeCommand, SecurityError};
use grub_distro_adapter::DistroProfile;
use grub_transaction_engine::{
    DiffReport, DiskSpaceError, LockDescriptor, SnapshotMeta, atomic_write, check_disk_space,
    check_package_manager_locks, create_snapshot, default_system_locks, generate_unified_diff,
    list_snapshots, restore_snapshot,
};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
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
    /// 包管理器互斥锁冲突
    PackageManagerLocked { message: String },
    /// 关键分区可用磁盘空间不足
    DiskSpaceInsufficient(DiskSpaceError),
    /// 快照创建失败
    SnapshotFailed { reason: String },
    /// 原子写入失败
    AtomicWriteFailed { reason: String },
    /// 安全策略检查失败（非白名单程序或参数注入风险）
    SecurityCheckFailed(SecurityError),
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
            DaemonError::PackageManagerLocked { message } => {
                write!(f, "无法执行引导修改，{}", message)
            }
            DaemonError::DiskSpaceInsufficient(err) => {
                write!(f, "{}", err)
            }
            DaemonError::SnapshotFailed { reason } => {
                write!(f, "创建配置快照失败，原因: {}", reason)
            }
            DaemonError::AtomicWriteFailed { reason } => {
                write!(f, "原子写入新配置失败，原因: {}", reason)
            }
            DaemonError::SecurityCheckFailed(err) => {
                write!(f, "{}", err)
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
    /// 是否跳过引导脚本语法检查（用于测试模拟）
    pub skip_syntax_check: bool,
    /// 引导生成任务设限超时秒数（默认为 60 秒）
    pub timeout_seconds: Option<u64>,
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
    /// 需检测的包管理器锁描述符列表
    pub lock_descriptors: Vec<LockDescriptor>,
}

impl GrubService {
    /// 创建标准系统服务实例
    pub fn new_system_default() -> Self {
        let distro_profile = DistroProfile::detect_current_system();
        Self {
            default_config_path: PathBuf::from("/etc/default/grub"),
            backup_dir: PathBuf::from("/var/backups/grub-manager"),
            distro_profile,
            lock_descriptors: default_system_locks(),
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
            lock_descriptors: Vec::new(),
        }
    }

    /// 链式配置自定义锁描述符列表（用于测试或非标准环境）
    pub fn with_lock_descriptors(mut self, descriptors: Vec<LockDescriptor>) -> Self {
        self.lock_descriptors = descriptors;
        self
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
        let start_time = Instant::now();
        let diff_summary = fs::read_to_string(&self.default_config_path)
            .ok()
            .map(|orig| {
                let diff = generate_unified_diff(&orig, new_config);
                let added = diff
                    .diff_text
                    .lines()
                    .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                    .count();
                let removed = diff
                    .diff_text
                    .lines()
                    .filter(|l| l.starts_with('-') && !l.starts_with("---"))
                    .count();
                format!("+{} / -{}", added, removed)
            });

        let res = (|| -> Result<TransactionResult, DaemonError> {
            if !self.default_config_path.exists() {
                return Err(DaemonError::ConfigNotFound {
                    path: self.default_config_path.clone(),
                });
            }

            // 检查包管理器并发互斥锁，避免与系统更新冲突
            check_package_manager_locks(&self.lock_descriptors).map_err(|e| {
                DaemonError::PackageManagerLocked {
                    message: e.to_string(),
                }
            })?;

            // 检查关键分区可用磁盘空间（要求至少 10MB 冗余，防止引导文件截断损坏）
            let min_disk_space_bytes = 10 * 1024 * 1024;
            check_disk_space(&self.default_config_path, min_disk_space_bytes)
                .map_err(DaemonError::DiskSpaceInsufficient)?;

            let target_boot_path = Path::new(&self.distro_profile.config_path);
            if target_boot_path.exists() {
                check_disk_space(target_boot_path, min_disk_space_bytes)
                    .map_err(DaemonError::DiskSpaceInsufficient)?;
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

            self.execute_update_with_rollback(&snapshot, options)
        })();

        let (success, snapshot_id) = match &res {
            Ok(r) => (r.success, Some(r.snapshot_id.clone())),
            Err(_) => (false, None),
        };

        record_audit_event(&AuditEvent {
            caller_uid: resolve_caller_uid(),
            action: AuditAction::ApplyChanges,
            reason: reason.to_string(),
            snapshot_id,
            success,
            duration: start_time.elapsed(),
            diff_summary,
        });

        res
    }

    /// 校验生成的引导脚本语法（若系统支持）
    fn verify_grub_script_syntax(&self, config_path: &Path) -> Result<(), String> {
        let cmd = SafeCommand::new(&self.distro_profile.check_command)
            .and_then(|c| c.args(&self.distro_profile.check_command_args))
            .and_then(|c| c.arg(&config_path.to_string_lossy()))
            .map_err(|e| format!("安全检查拦截: {}", e))?;

        let output = cmd.output().map_err(|e| {
            format!(
                "启动语法检查命令 '{}' 失败: {}",
                self.distro_profile.check_command, e
            )
        })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Err(format!("引导脚本语法校验未通过: {}", stderr.trim()))
        }
    }

    /// 执行引导更新命令并在失败或语法不通过时自动触发快照回滚
    fn execute_update_with_rollback(
        &self,
        snapshot: &SnapshotMeta,
        options: &TransactionOptions,
    ) -> Result<TransactionResult, DaemonError> {
        let cmd = match SafeCommand::new(&self.distro_profile.update_command)
            .and_then(|c| c.args(&self.distro_profile.command_args))
        {
            Ok(c) => c,
            Err(e) => {
                let _ = restore_snapshot(snapshot);
                return Err(DaemonError::SecurityCheckFailed(e));
            }
        };

        let timeout = std::time::Duration::from_secs(options.timeout_seconds.unwrap_or(60));
        match cmd.output_with_timeout(timeout) {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let combined_log = format!("{}\n{}", stdout, stderr);

                if output.status.success() {
                    // 语法预校验：验证生成的引导脚本合法性
                    if !options.skip_syntax_check {
                        let target_path = Path::new(&self.distro_profile.config_path);
                        if target_path.exists()
                            && let Err(syntax_err) = self.verify_grub_script_syntax(target_path)
                        {
                            warn!(
                                "引导脚本语法校验未通过，触发自动回滚，快照 ID: {}，原因: {}",
                                snapshot.id, syntax_err
                            );
                            let rollback_err = restore_snapshot(snapshot)
                                .err()
                                .map(|e| format!("且自动回滚失败: {}", e))
                                .unwrap_or_else(|| "已成功自动回滚至初始状态".to_string());

                            return Ok(TransactionResult {
                                success: false,
                                snapshot_id: snapshot.id.clone(),
                                log_output: format!("{}\n{}", combined_log, syntax_err),
                                error_message: Some(format!("{}, {}", syntax_err, rollback_err)),
                            });
                        }
                    }

                    info!(
                        "引导更新命令执行成功且语法校验通过，快照 ID: {}",
                        snapshot.id
                    );
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
                warn!("更新命令执行失败或超时，触发自动回滚，原因: {}", e);
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
        let start_time = Instant::now();
        let res = (|| -> Result<(), DaemonError> {
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
        })();

        record_audit_event(&AuditEvent {
            caller_uid: resolve_caller_uid(),
            action: AuditAction::RollbackSnapshot,
            reason: format!("还原至快照 {}", snapshot_id),
            snapshot_id: Some(snapshot_id.to_string()),
            success: res.is_ok(),
            duration: start_time.elapsed(),
            diff_summary: None,
        });

        res
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

    /// 通过 grubenv 快速设置默认启动项（微秒级生效，无需重写配置与重新生成引导脚本）
    ///
    /// # Errors
    /// 当包管理器被占用、命令执行失败或退出码非零时返回对应的 `DaemonError`。
    pub fn set_default_entry_fast(&self, entry_id_or_title: &str) -> Result<(), DaemonError> {
        let start_time = Instant::now();
        let res = (|| -> Result<(), DaemonError> {
            check_package_manager_locks(&self.lock_descriptors).map_err(|e| {
                DaemonError::PackageManagerLocked {
                    message: e.to_string(),
                }
            })?;

            debug!("开始通过 grubenv 快速设置默认引导项: {}", entry_id_or_title);

            let cmd = SafeCommand::new(&self.distro_profile.set_default_command)
                .and_then(|c| c.arg(entry_id_or_title))
                .map_err(DaemonError::SecurityCheckFailed)?;

            match cmd.output() {
                Ok(output) => {
                    if output.status.success() {
                        info!("通过 grubenv 成功设置默认启动项: {}", entry_id_or_title);
                        Ok(())
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        Err(DaemonError::CommandLaunchFailed {
                            command: self.distro_profile.set_default_command.clone(),
                            reason: format!(
                                "退出码非零 ({:?}): {}",
                                output.status.code(),
                                stderr.trim()
                            ),
                        })
                    }
                }
                Err(e) => Err(DaemonError::CommandLaunchFailed {
                    command: self.distro_profile.set_default_command.clone(),
                    reason: e.to_string(),
                }),
            }
        })();

        record_audit_event(&AuditEvent {
            caller_uid: resolve_caller_uid(),
            action: AuditAction::SetDefaultFast,
            reason: format!("设置为 {}", entry_id_or_title),
            snapshot_id: None,
            success: res.is_ok(),
            duration: start_time.elapsed(),
            diff_summary: None,
        });

        res
    }
}
