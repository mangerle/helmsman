use crate::audit::{AuditAction, AuditEvent, record_audit_event, resolve_caller_uid};
use crate::service::{DaemonError, GrubService, TransactionOptions, TransactionResult};
use grub_boot_reader::{CustomBootEntry, generate_custom_script, parse_custom_script};
use grub_transaction_engine::{atomic_write, check_disk_space, create_snapshot, prune_snapshots};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// 默认的受管自定义脚本路径
pub const DEFAULT_CUSTOM_SCRIPT_PATH: &str = "/etc/grub.d/41_helmsman_custom";

/// 默认的条目别名注册表存储路径
pub const DEFAULT_ALIASES_PATH: &str = "/etc/helmsman/aliases.json";

/// 自定义引导项与条目别名管理器
///
/// # 设计原理
/// - **实现初衷**：在完全不侵入发行版官方脚本（如 `10_linux`、`30_os-prober`）的前提下，
///   利用受管的 `/etc/grub.d/41_helmsman_custom` 独立存放用户自定义项（ISO 体验盘、EFI 链式加载等），
///   并通过独立的 JSON 注册表管理官方条目的友好别名映射，保证系统升级时 100% 安全。
/// - **事务保护**：自定义脚本的写入同样纳入原子写入、快照备份与失败回滚流程。
pub struct CustomManager {
    /// 受管自定义脚本路径
    pub custom_script_path: PathBuf,
    /// 别名存储文件路径
    pub aliases_path: PathBuf,
    /// 备份根目录
    pub backup_dir: PathBuf,
}

impl CustomManager {
    /// 创建基于系统默认路径的管理器实例
    pub fn new_system_default() -> Self {
        Self {
            custom_script_path: PathBuf::from(DEFAULT_CUSTOM_SCRIPT_PATH),
            aliases_path: PathBuf::from(DEFAULT_ALIASES_PATH),
            backup_dir: PathBuf::from("/var/backups/grub-manager"),
        }
    }

    /// 创建自定义路径实例（用于单元测试与集成测试）
    pub fn new_with_paths(
        custom_script_path: PathBuf,
        aliases_path: PathBuf,
        backup_dir: PathBuf,
    ) -> Self {
        Self {
            custom_script_path,
            aliases_path,
            backup_dir,
        }
    }

    /// 读取并解析所有自定义引导项
    pub fn load_custom_entries(&self) -> Result<Vec<CustomBootEntry>, DaemonError> {
        if !self.custom_script_path.exists() {
            debug!(
                "受管自定义脚本不存在: {}，返回空条目列表",
                self.custom_script_path.display()
            );
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&self.custom_script_path).map_err(|e| {
            DaemonError::ConfigReadFailed {
                path: self.custom_script_path.clone(),
                reason: e.to_string(),
            }
        })?;

        Ok(parse_custom_script(&content))
    }

    /// 保存自定义引导项列表至受管脚本（审计 UID 取自进程环境回退）
    ///
    /// # Errors
    /// 当目录创建、快照、原子写入或引导更新失败时返回对应的 `DaemonError`。
    pub fn save_custom_entries(
        &self,
        service: &GrubService,
        entries: &[CustomBootEntry],
        reason: &str,
        options: &TransactionOptions,
    ) -> Result<TransactionResult, DaemonError> {
        self.save_custom_entries_as(service, entries, reason, options, resolve_caller_uid())
    }

    /// 以显式调用方 UID 保存自定义引导项列表
    ///
    /// # Errors
    /// 当目录创建、快照、原子写入或引导更新失败时返回对应的 `DaemonError`。
    pub fn save_custom_entries_as(
        &self,
        service: &GrubService,
        entries: &[CustomBootEntry],
        reason: &str,
        options: &TransactionOptions,
        caller_uid: u32,
    ) -> Result<TransactionResult, DaemonError> {
        let start_time = Instant::now();
        let new_script_content =
            generate_custom_script(entries).map_err(|e| DaemonError::CustomEntryInvalid {
                reason: e.to_string(),
            })?;

        let res = (|| -> Result<TransactionResult, DaemonError> {
            // 确保父目录存在
            if let Some(parent) = self.custom_script_path.parent() {
                fs::create_dir_all(parent).map_err(|e| DaemonError::AtomicWriteFailed {
                    reason: format!("创建目录 '{}' 失败: {}", parent.display(), e),
                })?;
            }

            // 检查可用磁盘空间
            check_disk_space(&self.custom_script_path, 10 * 1024 * 1024)
                .map_err(DaemonError::DiskSpaceInsufficient)?;

            // 若已有文件则创建快照
            let snapshot_id = if self.custom_script_path.exists() {
                let snapshot = create_snapshot(&self.custom_script_path, &self.backup_dir, reason)
                    .map_err(|e| DaemonError::SnapshotFailed {
                        reason: e.to_string(),
                    })?;
                // 有界保留快照，防止备份目录无限膨胀
                if let Err(e) = prune_snapshots(&self.backup_dir, 20) {
                    tracing::warn!("裁剪历史快照失败，原因: {}", e);
                }
                snapshot.id
            } else {
                "initial_custom_created".to_string()
            };

            // 原子替换写入
            atomic_write(&self.custom_script_path, &new_script_content).map_err(|e| {
                DaemonError::AtomicWriteFailed {
                    reason: e.to_string(),
                }
            })?;

            // 在 Unix 平台下确保可执行权限 (0755)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let permissions = fs::Permissions::from_mode(0o755);
                let _ = fs::set_permissions(&self.custom_script_path, permissions);
            }

            info!(
                "自定义引导脚本写入成功: {}，条目数: {}",
                self.custom_script_path.display(),
                entries.len()
            );

            if options.skip_command_execution {
                return Ok(TransactionResult {
                    success: true,
                    snapshot_id,
                    log_output: "跳过引导更新命令执行（测试模式）".to_string(),
                    error_message: None,
                });
            }

            // 若不需要跳过则触发引导更新（沿用调用方 UID 以保持审计一致）
            let update_res = service.apply_changes_as(
                &fs::read_to_string(&service.default_config_path).unwrap_or_default(),
                reason,
                options,
                caller_uid,
            )?;
            Ok(update_res)
        })();

        let success = res.as_ref().map(|r| r.success).unwrap_or(false);
        record_audit_event(&AuditEvent {
            caller_uid,
            action: AuditAction::ApplyCustomEntries,
            reason: format!("更新自定义条目: {}", reason),
            snapshot_id: res.as_ref().ok().map(|r| r.snapshot_id.clone()),
            success,
            duration: start_time.elapsed(),
            diff_summary: Some(format!("条目数: {}", entries.len())),
        });

        res
    }

    /// 读取条目别名映射表
    pub fn load_aliases(&self) -> HashMap<String, String> {
        if !self.aliases_path.exists() {
            return HashMap::new();
        }

        let content = match fs::read_to_string(&self.aliases_path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        serde_json::from_str(&content).unwrap_or_default()
    }

    /// 保存条目别名映射
    pub fn set_alias(&self, entry_id: &str, alias: &str) -> Result<(), DaemonError> {
        let mut aliases = self.load_aliases();
        let trimmed_alias = alias.trim();

        if trimmed_alias.is_empty() {
            aliases.remove(entry_id);
        } else {
            aliases.insert(entry_id.to_string(), trimmed_alias.to_string());
        }

        if let Some(parent) = self.aliases_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let json_str =
            serde_json::to_string_pretty(&aliases).map_err(|e| DaemonError::AtomicWriteFailed {
                reason: format!("序列化别名注册表失败: {}", e),
            })?;

        atomic_write(&self.aliases_path, &json_str).map_err(|e| {
            DaemonError::AtomicWriteFailed {
                reason: format!("保存别名注册表失败: {}", e),
            }
        })?;

        info!(
            "成功更新条目别名，条目 ID: '{}' -> 别名: '{}'",
            entry_id, trimmed_alias
        );

        record_audit_event(&AuditEvent {
            caller_uid: resolve_caller_uid(),
            action: AuditAction::SetAlias,
            reason: format!("条目 '{}' 别名更新为 '{}'", entry_id, trimmed_alias),
            snapshot_id: None,
            success: true,
            duration: Duration::ZERO,
            diff_summary: None,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grub_distro_adapter::{DistroFamily, DistroProfile, FirmwareType};

    fn get_custom_test_env(name: &str) -> (PathBuf, CustomManager, GrubService) {
        let base = std::env::temp_dir().join("helmsman_custom_test").join(name);
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        let custom_script = base.join("41_helmsman_custom");
        let aliases_file = base.join("aliases.json");
        let backup_dir = base.join("backups");
        fs::create_dir_all(&backup_dir).unwrap();

        let default_config = base.join("default_grub");
        fs::write(&default_config, "GRUB_DEFAULT=0\n").unwrap();

        let distro_profile = DistroProfile {
            family: DistroFamily::DebianUbuntu,
            name: "Test Distro".to_string(),
            firmware: FirmwareType::Uefi,
            config_path: "/boot/grub/grub.cfg".to_string(),
            update_command: "true".to_string(),
            command_args: Vec::new(),
            check_command: "true".to_string(),
            check_command_args: Vec::new(),
            grubenv_path: "/boot/grub/grubenv".to_string(),
            set_default_command: "true".to_string(),
        };

        let service =
            GrubService::new_with_paths(default_config, backup_dir.clone(), distro_profile);
        let manager = CustomManager::new_with_paths(custom_script, aliases_file, backup_dir);
        (base, manager, service)
    }

    #[test]
    fn test_custom_manager_lifecycle() {
        let (_base, manager, service) = get_custom_test_env("lifecycle");

        // 1. 初始应为空
        let entries = manager.load_custom_entries().unwrap();
        assert!(entries.is_empty());

        // 2. 添加并保存自定义项
        let item1 = CustomBootEntry::new_iso_boot(
            "iso_1",
            "Ubuntu Live ISO",
            "/boot/iso/ubuntu.iso",
            "UUID-1234",
            "",
        );
        let options = TransactionOptions {
            skip_command_execution: true,
            ..Default::default()
        };
        let res = manager
            .save_custom_entries(&service, &[item1], "测试保存 ISO", &options)
            .unwrap();
        assert!(res.success);

        // 3. 重新读取验证
        let reloaded = manager.load_custom_entries().unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].id, "iso_1");
        assert_eq!(reloaded[0].title, "Ubuntu Live ISO");

        // 4. 别名管理测试
        manager
            .set_alias("gnulinux-6.8", "Ubuntu 6.8 (生产)")
            .unwrap();
        let aliases = manager.load_aliases();
        assert_eq!(aliases.get("gnulinux-6.8").unwrap(), "Ubuntu 6.8 (生产)");

        // 5. 清除别名
        manager.set_alias("gnulinux-6.8", "").unwrap();
        let aliases_cleared = manager.load_aliases();
        assert!(!aliases_cleared.contains_key("gnulinux-6.8"));
    }
}
