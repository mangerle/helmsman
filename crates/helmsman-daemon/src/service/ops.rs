use crate::audit::{AuditAction, resolve_caller_uid, with_audit};
use crate::executor::SafeCommand;
use crate::service::{DaemonError, GrubService, TransactionOptions};
use grub_config_parser::parse_grub_config;
use grub_transaction_engine::{
    check_package_manager_locks, delete_snapshot, diff_snapshot_against_target, export_snapshot,
    list_snapshots, prune_snapshots, read_snapshot_content, restore_snapshot,
};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// 快照回滚与默认项切换等系统运维操作
impl GrubService {
    /// 一键回滚至指定快照（审计 UID 取自进程环境回退）
    ///
    /// # Errors
    /// 当快照列表无法读取、未找到 ID 或还原失败时返回对应的 [`DaemonError`]。
    pub fn rollback_to_snapshot(&self, snapshot_id: &str) -> Result<(), DaemonError> {
        self.rollback_to_snapshot_as(snapshot_id, resolve_caller_uid())
    }

    /// 以显式调用方 UID 回滚至指定快照，并重新生成引导配置
    ///
    /// # Errors
    /// 当快照列表无法读取、未找到 ID 或还原失败时返回对应的 [`DaemonError`]。
    pub fn rollback_to_snapshot_as(
        &self,
        snapshot_id: &str,
        caller_uid: u32,
    ) -> Result<(), DaemonError> {
        let _write_guard = self.acquire_write_lock();
        with_audit(
            caller_uid,
            AuditAction::RollbackSnapshot,
            format!("还原至快照 {}", snapshot_id),
            Some(snapshot_id.to_string()),
            None,
            |_| (true, None),
            || {
                let snapshots = list_snapshots(&self.backup_dir).map_err(|e| {
                    DaemonError::SnapshotListFailed {
                        reason: e.to_string(),
                    }
                })?;

                let target_snapshot = snapshots
                    .into_iter()
                    .find(|s| s.id == snapshot_id)
                    .ok_or_else(|| DaemonError::SnapshotNotFound {
                        id: snapshot_id.to_string(),
                    })?;

                restore_snapshot(&target_snapshot).map_err(|e| {
                    DaemonError::SnapshotRestoreFailed {
                        id: snapshot_id.to_string(),
                        reason: e.to_string(),
                    }
                })?;

                // 还原配置后必须重新生成 grub.cfg，否则磁盘配置与引导脚本不一致
                let rebuild_cmd = SafeCommand::new(&self.distro_profile.update_command)
                    .and_then(|c| c.args(&self.distro_profile.command_args))
                    .map_err(DaemonError::SecurityCheckFailed)?;

                let rebuild_output = rebuild_cmd
                    .output_with_timeout(std::time::Duration::from_secs(60))
                    .map_err(|e| DaemonError::CommandLaunchFailed {
                        command: self.distro_profile.update_command.clone(),
                        reason: format!("快照已还原但引导重建失败: {e}"),
                    })?;

                if !rebuild_output.status.success() {
                    let stderr = String::from_utf8_lossy(&rebuild_output.stderr);
                    return Err(DaemonError::CommandLaunchFailed {
                        command: self.distro_profile.update_command.clone(),
                        reason: format!(
                            "快照已还原但引导重建退出码非零 ({:?}): {}",
                            rebuild_output.status.code(),
                            stderr.trim()
                        ),
                    });
                }

                info!("成功还原历史快照并重建引导配置: {}", snapshot_id);
                Ok(())
            },
        )
    }

    /// 删除指定历史快照（审计 UID 取自进程环境回退）
    ///
    /// # Errors
    /// 未找到快照或删除失败时返回对应的 [`DaemonError`]。
    pub fn delete_snapshot(&self, snapshot_id: &str) -> Result<(), DaemonError> {
        self.delete_snapshot_as(snapshot_id, resolve_caller_uid())
    }

    /// 以显式调用方 UID 删除快照
    ///
    /// # Errors
    /// 未找到快照或删除失败时返回对应的 [`DaemonError`]。
    pub fn delete_snapshot_as(
        &self,
        snapshot_id: &str,
        caller_uid: u32,
    ) -> Result<(), DaemonError> {
        let _write_guard = self.acquire_write_lock();
        with_audit(
            caller_uid,
            AuditAction::DeleteSnapshot,
            format!("删除快照 {snapshot_id}"),
            Some(snapshot_id.to_string()),
            None,
            |_| (true, None),
            || {
                delete_snapshot(&self.backup_dir, snapshot_id).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        DaemonError::SnapshotNotFound {
                            id: snapshot_id.to_string(),
                        }
                    } else {
                        DaemonError::SnapshotRestoreFailed {
                            id: snapshot_id.to_string(),
                            reason: e.to_string(),
                        }
                    }
                })
            },
        )
    }

    /// 将快照导出到指定路径（审计 UID 取自进程环境回退）
    ///
    /// # Errors
    /// 未找到快照、目标路径非法或复制失败时返回对应的 [`DaemonError`]。
    pub fn export_snapshot(
        &self,
        snapshot_id: &str,
        dest_path: &Path,
    ) -> Result<PathBuf, DaemonError> {
        self.export_snapshot_as(snapshot_id, dest_path, resolve_caller_uid())
    }

    /// 以显式调用方 UID 导出快照
    ///
    /// # Errors
    /// 未找到快照、目标路径非法或复制失败时返回对应的 [`DaemonError`]。
    pub fn export_snapshot_as(
        &self,
        snapshot_id: &str,
        dest_path: &Path,
        caller_uid: u32,
    ) -> Result<PathBuf, DaemonError> {
        with_audit(
            caller_uid,
            AuditAction::ExportSnapshot,
            format!("导出快照 {snapshot_id} 至 {}", dest_path.display()),
            Some(snapshot_id.to_string()),
            None,
            |_| (true, None),
            || {
                export_snapshot(&self.backup_dir, snapshot_id, dest_path).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        DaemonError::SnapshotNotFound {
                            id: snapshot_id.to_string(),
                        }
                    } else {
                        DaemonError::AtomicWriteFailed {
                            reason: format!("导出快照 '{snapshot_id}' 失败: {e}"),
                        }
                    }
                })
            },
        )
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

    /// 预览指定快照与当前目标文件的差异（回滚前确认）
    ///
    /// # Errors
    /// 未找到快照或备份读取失败时返回对应的 [`DaemonError`]。
    pub fn preview_snapshot_diff(
        &self,
        snapshot_id: &str,
    ) -> Result<grub_transaction_engine::DiffReport, DaemonError> {
        diff_snapshot_against_target(&self.backup_dir, snapshot_id).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DaemonError::SnapshotNotFound {
                    id: snapshot_id.to_string(),
                }
            } else {
                DaemonError::SnapshotListFailed {
                    reason: format!("生成快照 '{snapshot_id}' 差异预览失败: {e}"),
                }
            }
        })
    }

    /// 读取指定快照的备份内容原文
    ///
    /// # Errors
    /// 未找到快照或备份读取失败时返回对应的 [`DaemonError`]。
    pub fn get_snapshot_content(&self, snapshot_id: &str) -> Result<String, DaemonError> {
        read_snapshot_content(&self.backup_dir, snapshot_id).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DaemonError::SnapshotNotFound {
                    id: snapshot_id.to_string(),
                }
            } else {
                DaemonError::SnapshotListFailed {
                    reason: format!("读取快照 '{snapshot_id}' 内容失败: {e}"),
                }
            }
        })
    }

    /// 按保留上限有界裁剪历史快照，返回删除数量
    ///
    /// # Errors
    /// 当备份目录读取或删除失败时返回 [`DaemonError::SnapshotListFailed`]。
    pub fn prune_snapshot_history(&self, max_keep: usize) -> Result<usize, DaemonError> {
        let _write_guard = self.acquire_write_lock();
        prune_snapshots(&self.backup_dir, max_keep).map_err(|e| DaemonError::SnapshotListFailed {
            reason: format!("裁剪历史快照失败（保留上限 {max_keep}）: {e}"),
        })
    }

    /// 确保配置处于 saved 记忆模式（GRUB_DEFAULT=saved）
    ///
    /// # 设计原理
    /// - **实现初衷**：`grub-set-default` 只写 grubenv 的 `saved_entry`，若 `GRUB_DEFAULT` 仍为
    ///   数字索引或固定标题则切换不会生效。必须先原子切换到 saved 模式再写入目标项。
    /// - **核心优势**：复用既有事务（快照 + 原子写 + 引导重建），失败可回滚。
    fn ensure_saved_default_mode(
        &self,
        options: &TransactionOptions,
        caller_uid: u32,
    ) -> Result<(), DaemonError> {
        let current = fs::read_to_string(&self.default_config_path).map_err(|e| {
            DaemonError::ConfigReadFailed {
                path: self.default_config_path.clone(),
                reason: e.to_string(),
            }
        })?;

        let mut config = parse_grub_config(&current);
        if config.get("GRUB_DEFAULT") == Some("saved") {
            return Ok(());
        }

        config.set("GRUB_DEFAULT", "saved");
        config.set("GRUB_SAVEDEFAULT", "true");
        let new_config = config.serialize();

        let result = self.apply_changes_unlocked(
            &new_config,
            "自动启用 saved 记忆模式以支持快速切换默认项",
            options,
            caller_uid,
        )?;

        if !result.success {
            return Err(DaemonError::CommandLaunchFailed {
                command: self.distro_profile.update_command.clone(),
                reason: format!(
                    "启用 saved 记忆模式失败: {}",
                    result
                        .error_message
                        .unwrap_or_else(|| "引导更新未成功".to_string())
                ),
            });
        }
        Ok(())
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
        let _write_guard = self.acquire_write_lock();
        with_audit(
            caller_uid,
            AuditAction::SetDefaultFast,
            format!("设置为 {}", entry_id_or_title),
            None,
            None,
            |_| (true, None),
            || {
                check_package_manager_locks(&self.lock_descriptors).map_err(|e| {
                    DaemonError::PackageManagerLocked {
                        message: e.to_string(),
                    }
                })?;

                debug!("开始通过 grubenv 快速设置默认引导项: {}", entry_id_or_title);

                // grub-set-default 仅在 GRUB_DEFAULT=saved 时生效，必要时先切换到记忆模式
                self.ensure_saved_default_mode(&TransactionOptions::default(), caller_uid)?;

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
            },
        )
    }
}
