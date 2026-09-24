use crate::audit::{AuditAction, AuditEvent, record_audit_event, resolve_caller_uid};
use crate::executor::SafeCommand;
use crate::service::{DaemonError, GrubService, TransactionOptions};
use grub_config_parser::parse_grub_config;
use grub_transaction_engine::{
    InstalledThemeInfo, check_package_manager_locks, delete_snapshot, diff_snapshot_against_target,
    export_snapshot, list_installed_themes, list_snapshots, prune_snapshots, read_snapshot_content,
    remove_theme, restore_snapshot,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{debug, info};

/// 主题安装、快照回滚与默认项切换等系统运维操作
impl GrubService {
    /// 校验主题压缩包源路径是否位于允许的用户可读沙箱目录内
    ///
    /// # 设计原理
    /// - **实现初衷**：D-Bus 客户端可传入任意本地路径，守护进程以 root 读取会造成任意文件读取与大文件 DoS。
    /// - **核心优势**：仅接受临时目录、用户主目录与运行时目录下的普通文件，并限制体积上限。
    /// - **代价与局限**：不支持通过 FileDescriptor/portal 直传，调用方需先把压缩包落到允许目录。
    fn validate_theme_archive_source(path: &Path, caller_uid: u32) -> Result<(), DaemonError> {
        let canonical = path
            .canonicalize()
            .map_err(|e| DaemonError::ThemeInstallFailed {
                reason: format!("主题压缩包路径无法解析 '{}': {}", path.display(), e),
            })?;

        let meta = std::fs::metadata(&canonical).map_err(|e| DaemonError::ThemeInstallFailed {
            reason: format!("无法读取主题压缩包元数据 '{}': {}", canonical.display(), e),
        })?;

        if !meta.is_file() {
            return Err(DaemonError::ThemeInstallFailed {
                reason: format!("主题压缩包不是普通文件: {}", canonical.display()),
            });
        }

        const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
        if meta.len() > MAX_ARCHIVE_BYTES {
            return Err(DaemonError::ThemeInstallFailed {
                reason: format!(
                    "主题压缩包过大 ({} 字节)，上限 {} 字节",
                    meta.len(),
                    MAX_ARCHIVE_BYTES
                ),
            });
        }

        let mut allowed_roots: Vec<std::path::PathBuf> = Vec::with_capacity(4);
        allowed_roots.push(std::env::temp_dir());
        allowed_roots.push(std::path::PathBuf::from("/tmp"));
        allowed_roots.push(std::path::PathBuf::from("/var/tmp"));
        allowed_roots.push(std::path::PathBuf::from(format!("/run/user/{caller_uid}")));
        if let Ok(home) = std::env::var("HOME") {
            allowed_roots.push(std::path::PathBuf::from(home));
        }

        // Windows 下 canonicalize 会得到 \\?\ 前缀路径，允许根目录同样规范化后再比较
        let under_allowed = allowed_roots.iter().any(|root| {
            let root_canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
            canonical.starts_with(root_canonical)
        });
        if !under_allowed {
            return Err(DaemonError::ThemeInstallFailed {
                reason: format!(
                    "主题压缩包必须位于临时目录、用户主目录或运行时目录内，当前路径: {}",
                    canonical.display()
                ),
            });
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let owner = meta.uid();
            if owner != 0 && owner != caller_uid {
                return Err(DaemonError::ThemeInstallFailed {
                    reason: format!(
                        "主题压缩包属主 UID {} 与调用方 UID {} 不一致: {}",
                        owner,
                        caller_uid,
                        canonical.display()
                    ),
                });
            }
        }

        Ok(())
    }

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
        Self::validate_theme_archive_source(archive_path, caller_uid).inspect_err(|e| {
            let err_msg = e.to_string();
            record_audit_event(&AuditEvent {
                caller_uid,
                action: AuditAction::InstallTheme,
                reason: format!("主题安装路径校验失败: {}", err_msg),
                snapshot_id: None,
                success: false,
                duration: start_time.elapsed(),
                diff_summary: None,
            });
        })?;

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

    /// 以显式调用方 UID 回滚至指定快照，并重新生成引导配置
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

    /// 列出已安装的 GRUB 主题
    ///
    /// # Errors
    /// 主题目录读取失败时返回 [`DaemonError::ThemeInstallFailed`]。
    pub fn list_themes(&self) -> Result<Vec<InstalledThemeInfo>, DaemonError> {
        list_installed_themes(&self.themes_dir).map_err(|e| DaemonError::ThemeInstallFailed {
            reason: e.to_string(),
        })
    }

    /// 卸载指定名称的主题（审计 UID 取自进程环境回退）
    ///
    /// # Errors
    /// 名称非法、路径穿越或删除失败时返回对应的 [`DaemonError`]。
    pub fn remove_theme(&self, theme_name: &str) -> Result<(), DaemonError> {
        self.remove_theme_as(theme_name, resolve_caller_uid())
    }

    /// 以显式调用方 UID 卸载主题
    ///
    /// # Errors
    /// 名称非法、路径穿越或删除失败时返回对应的 [`DaemonError`]。
    pub fn remove_theme_as(&self, theme_name: &str, caller_uid: u32) -> Result<(), DaemonError> {
        let start_time = Instant::now();
        let res = remove_theme(&self.themes_dir, theme_name).map_err(|e| {
            DaemonError::ThemeInstallFailed {
                reason: format!("卸载主题 '{theme_name}' 失败: {e}"),
            }
        });

        record_audit_event(&AuditEvent {
            caller_uid,
            action: AuditAction::RemoveTheme,
            reason: format!("卸载主题 '{theme_name}'"),
            snapshot_id: None,
            success: res.is_ok(),
            duration: start_time.elapsed(),
            diff_summary: None,
        });
        res
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
        let start_time = Instant::now();
        let res = delete_snapshot(&self.backup_dir, snapshot_id).map_err(|e| {
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
        });

        record_audit_event(&AuditEvent {
            caller_uid,
            action: AuditAction::DeleteSnapshot,
            reason: format!("删除快照 {snapshot_id}"),
            snapshot_id: Some(snapshot_id.to_string()),
            success: res.is_ok(),
            duration: start_time.elapsed(),
            diff_summary: None,
        });
        res
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
        let start_time = Instant::now();
        let res = export_snapshot(&self.backup_dir, snapshot_id, dest_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DaemonError::SnapshotNotFound {
                    id: snapshot_id.to_string(),
                }
            } else {
                DaemonError::AtomicWriteFailed {
                    reason: format!("导出快照 '{snapshot_id}' 失败: {e}"),
                }
            }
        });

        record_audit_event(&AuditEvent {
            caller_uid,
            action: AuditAction::ExportSnapshot,
            reason: format!("导出快照 {snapshot_id} 至 {}", dest_path.display()),
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
    fn ensure_saved_default_mode(&self, options: &TransactionOptions) -> Result<(), DaemonError> {
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

        let result = self.apply_changes_as(
            &new_config,
            "自动启用 saved 记忆模式以支持快速切换默认项",
            options,
            resolve_caller_uid(),
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
        let start_time = Instant::now();
        let res = (|| -> Result<(), DaemonError> {
            check_package_manager_locks(&self.lock_descriptors).map_err(|e| {
                DaemonError::PackageManagerLocked {
                    message: e.to_string(),
                }
            })?;

            debug!("开始通过 grubenv 快速设置默认引导项: {}", entry_id_or_title);

            // grub-set-default 仅在 GRUB_DEFAULT=saved 时生效，必要时先切换到记忆模式
            self.ensure_saved_default_mode(&TransactionOptions::default())?;

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
