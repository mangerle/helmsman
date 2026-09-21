use crate::audit::{AuditAction, AuditEvent, record_audit_event, resolve_caller_uid};
use crate::executor::SafeCommand;
use crate::service::{DaemonError, GrubService};
use grub_transaction_engine::{check_package_manager_locks, list_snapshots, restore_snapshot};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, info};

/// 主题安装、快照回滚与默认项切换等系统运维操作
impl GrubService {
    /// 从压缩包安全安装主题至系统主题目录（审计 UID 取自进程环境回退）
    ///
    /// # Errors
    /// 当压缩包校验失败、路径穿越或落盘失败时返回 [`DaemonError::ThemeInstallFailed`]。
    pub fn install_theme_archive(
        &self,
        archive_path: &Path,
        theme_name: Option<&str>,
    ) -> Result<PathBuf, DaemonError> {
        self.install_theme_archive_as(archive_path, theme_name, resolve_caller_uid())
    }

    /// 以显式调用方 UID 从压缩包安全安装主题
    ///
    /// # Errors
    /// 当压缩包校验失败、路径穿越或落盘失败时返回 [`DaemonError::ThemeInstallFailed`]。
    pub fn install_theme_archive_as(
        &self,
        archive_path: &Path,
        theme_name: Option<&str>,
        caller_uid: u32,
    ) -> Result<PathBuf, DaemonError> {
        let start_time = Instant::now();
        let installed = grub_transaction_engine::install_theme_from_archive(
            archive_path,
            &self.themes_dir,
            theme_name,
        )
        .map_err(|e| {
            let err_msg = e.to_string();
            record_audit_event(&AuditEvent {
                caller_uid,
                action: AuditAction::InstallTheme,
                reason: format!("主题安装失败: {}", err_msg),
                snapshot_id: None,
                success: false,
                duration: start_time.elapsed(),
                diff_summary: None,
            });
            DaemonError::ThemeInstallFailed { reason: err_msg }
        })?;

        record_audit_event(&AuditEvent {
            caller_uid,
            action: AuditAction::InstallTheme,
            reason: format!("成功从压缩包安装主题至: {}", installed.display()),
            snapshot_id: None,
            success: true,
            duration: start_time.elapsed(),
            diff_summary: None,
        });

        Ok(installed)
    }

    /// 一键回滚至指定快照（审计 UID 取自进程环境回退）
    ///
    /// # Errors
    /// 当快照列表无法读取、未找到 ID 或还原失败时返回对应的 [`DaemonError`]。
    pub fn rollback_to_snapshot(&self, snapshot_id: &str) -> Result<(), DaemonError> {
        self.rollback_to_snapshot_as(snapshot_id, resolve_caller_uid())
    }

    /// 以显式调用方 UID 回滚至指定快照
    ///
    /// # Errors
    /// 当快照列表无法读取、未找到 ID 或还原失败时返回对应的 [`DaemonError`]。
    pub fn rollback_to_snapshot_as(
        &self,
        snapshot_id: &str,
        caller_uid: u32,
    ) -> Result<(), DaemonError> {
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
            caller_uid,
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
    /// 当备份目录无法访问时返回 [`DaemonError::SnapshotListFailed`]。
    pub fn get_available_snapshots(
        &self,
    ) -> Result<Vec<grub_transaction_engine::SnapshotMeta>, DaemonError> {
        list_snapshots(&self.backup_dir).map_err(|e| DaemonError::SnapshotListFailed {
            reason: e.to_string(),
        })
    }

    /// 通过 grubenv 快速设置默认启动项（审计 UID 取自进程环境回退）
    ///
    /// # Errors
    /// 当包管理器被占用、命令执行失败或退出码非零时返回对应的 [`DaemonError`]。
    pub fn set_default_entry_fast(&self, entry_id_or_title: &str) -> Result<(), DaemonError> {
        self.set_default_entry_fast_as(entry_id_or_title, resolve_caller_uid())
    }

    /// 以显式调用方 UID 通过 grubenv 快速设置默认启动项
    ///
    /// # Errors
    /// 当包管理器被占用、命令执行失败或退出码非零时返回对应的 [`DaemonError`]。
    pub fn set_default_entry_fast_as(
        &self,
        entry_id_or_title: &str,
        caller_uid: u32,
    ) -> Result<(), DaemonError> {
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
            caller_uid,
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
