use crate::audit::{AuditAction, AuditEvent, record_audit_event, resolve_caller_uid, with_audit};
use crate::service::{
    DEFAULT_BACKUP_DIR, DEFAULT_SNAPSHOT_RETENTION, DaemonError, GrubService,
    MIN_REQUIRED_DISK_SPACE_BYTES, TransactionOptions, TransactionResult,
};
use grub_boot_reader::{CustomBootEntry, generate_custom_script, parse_custom_script};
use grub_transaction_engine::{
    atomic_write, check_disk_space, create_snapshot, prune_snapshots, restore_snapshot,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
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
    /// 进程内写操作互斥锁（锁序先于 GrubService::write_lock，防止并发写竞态与死锁）
    write_lock: Mutex<()>,
}

impl CustomManager {
    /// 获取写操作互斥锁（仅在同步事务内持有）
    pub(crate) fn acquire_write_lock(&self) -> MutexGuard<'_, ()> {
        self.write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 创建基于系统默认路径的管理器实例
    pub fn new_system_default() -> Self {
        Self {
            custom_script_path: PathBuf::from(DEFAULT_CUSTOM_SCRIPT_PATH),
            aliases_path: PathBuf::from(DEFAULT_ALIASES_PATH),
            backup_dir: PathBuf::from(DEFAULT_BACKUP_DIR),
            write_lock: Mutex::new(()),
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
            write_lock: Mutex::new(()),
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
    /// 流程：整表冲突校验 → 生成脚本并做结构校验 → 快照 → 原子写入 → 触发引导更新；
    /// 任一后置步骤失败时自动将自定义脚本回滚至写入前状态。
    ///
    /// # Errors
    /// 当条目冲突/非法、快照、原子写入或引导更新失败时返回对应的 `DaemonError`。
    pub fn save_custom_entries_as(
        &self,
        service: &GrubService,
        entries: &[CustomBootEntry],
        reason: &str,
        options: &TransactionOptions,
        caller_uid: u32,
    ) -> Result<TransactionResult, DaemonError> {
        let _custom_guard = self.acquire_write_lock();
        // 冲突检测 + 字段校验 + 脚本结构校验，任一失败均不落盘
        let new_script_content =
            generate_custom_script(entries).map_err(|e| DaemonError::CustomEntryInvalid {
                reason: e.to_string(),
            })?;

        with_audit(
            caller_uid,
            AuditAction::ApplyCustomEntries,
            format!("更新自定义条目: {}", reason),
            None,
            Some(format!("条目数: {}", entries.len())),
            |r: &TransactionResult| (r.success, Some(r.snapshot_id.clone())),
            || -> Result<TransactionResult, DaemonError> {
                // 确保父目录存在
                if let Some(parent) = self.custom_script_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| DaemonError::AtomicWriteFailed {
                        reason: format!("创建目录 '{}' 失败: {}", parent.display(), e),
                    })?;
                }

                // 检查可用磁盘空间
                check_disk_space(&self.custom_script_path, MIN_REQUIRED_DISK_SPACE_BYTES)
                    .map_err(DaemonError::DiskSpaceInsufficient)?;

                // 若已有文件则创建快照，失败或更新失败时用于回滚自定义脚本本身
                let mut script_snapshot = None;
                let snapshot_id = if self.custom_script_path.exists() {
                    let snapshot =
                        create_snapshot(&self.custom_script_path, &self.backup_dir, reason)
                            .map_err(|e| DaemonError::SnapshotFailed {
                                reason: e.to_string(),
                            })?;
                    // 有界保留快照，防止备份目录无限膨胀
                    if let Err(e) = prune_snapshots(&self.backup_dir, DEFAULT_SNAPSHOT_RETENTION) {
                        tracing::warn!("裁剪历史快照失败，原因: {}", e);
                    }
                    let id = snapshot.id.clone();
                    script_snapshot = Some(snapshot);
                    id
                } else {
                    "initial_custom_created".to_string()
                };

                // 原子替换写入
                if let Err(e) = atomic_write(&self.custom_script_path, &new_script_content) {
                    return Err(DaemonError::AtomicWriteFailed {
                        reason: e.to_string(),
                    });
                }

                // 在 Unix 平台下确保可执行权限 (0755)
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let permissions = fs::Permissions::from_mode(0o755);
                    if let Err(e) = fs::set_permissions(&self.custom_script_path, permissions) {
                        rollback_custom_script(&self.custom_script_path, script_snapshot.as_ref());
                        return Err(DaemonError::AtomicWriteFailed {
                            reason: format!(
                                "设置自定义脚本可执行权限失败 '{}': {}",
                                self.custom_script_path.display(),
                                e
                            ),
                        });
                    }
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

                // 读取当前 /etc/default/grub，禁止吞错传空串以免覆盖系统配置
                let default_cfg_content = match fs::read_to_string(&service.default_config_path) {
                    Ok(content) => content,
                    Err(e) => {
                        rollback_custom_script(&self.custom_script_path, script_snapshot.as_ref());
                        return Err(DaemonError::ConfigReadFailed {
                            path: service.default_config_path.clone(),
                            reason: e.to_string(),
                        });
                    }
                };

                // 若不需要跳过则触发引导更新（沿用调用方 UID 以保持审计一致）
                let update_res =
                    service.apply_changes_as(&default_cfg_content, reason, options, caller_uid);

                // 引导更新失败时回滚自定义脚本，避免磁盘脚本与生成的 grub.cfg 不一致
                match update_res {
                    Ok(result) => Ok(result),
                    Err(err) => {
                        rollback_custom_script(&self.custom_script_path, script_snapshot.as_ref());
                        Err(err)
                    }
                }
            },
        )
    }

    /// 删除指定自定义引导项并落盘（仅限受管自定义项）
    ///
    /// # 设计原理
    /// - **实现初衷**：删除只针对 Helmsman 拥有生命周期的 `41_helmsman_custom` 条目；
    ///   系统内核、Memtest、os-prober 条目由发行版脚本生成，**禁止物理删除**对应文件，
    ///   否则内核升级或包管理器会立刻暴露缺口甚至无法启动。
    /// - **核心优势**：删除 = 从受管列表移除后重新生成脚本，天然纳入快照与失败回滚。
    /// - **代价与局限**：无法“删除”系统生成条目；此类需求只能走类级隐藏或未来单项过滤。
    ///
    /// # Errors
    /// 未找到目标 ID、条目校验失败或写盘/更新失败时返回对应的 [`DaemonError`]。
    pub fn delete_custom_entry(
        &self,
        service: &GrubService,
        entry_id: &str,
        reason: &str,
        options: &TransactionOptions,
    ) -> Result<TransactionResult, DaemonError> {
        let mut entries = self.load_custom_entries()?;
        let before = entries.len();
        entries.retain(|e| e.id != entry_id);
        if entries.len() == before {
            return Err(DaemonError::CustomEntryInvalid {
                reason: format!(
                    "未找到 ID 为 '{}' 的自定义引导项；系统生成条目不支持删除，仅可隐藏",
                    entry_id
                ),
            });
        }
        self.save_custom_entries(service, &entries, reason, options)
    }

    /// 尝试读取条目别名映射表，区分不存在与读取/解析失败
    fn try_load_aliases(&self) -> Result<HashMap<String, String>, DaemonError> {
        if !self.aliases_path.exists() {
            return Ok(HashMap::new());
        }

        let content =
            fs::read_to_string(&self.aliases_path).map_err(|e| DaemonError::ConfigReadFailed {
                path: self.aliases_path.clone(),
                reason: e.to_string(),
            })?;

        serde_json::from_str(&content).map_err(|e| DaemonError::ConfigReadFailed {
            path: self.aliases_path.clone(),
            reason: format!("解析别名注册表 JSON 失败: {}", e),
        })
    }

    /// 读取条目别名映射表
    ///
    /// # 语义说明
    /// 别名是**界面显示名**，不会写入 `grub.cfg`，也不改变开机菜单 `menuentry` 标题。
    pub fn load_aliases(&self) -> HashMap<String, String> {
        self.try_load_aliases().unwrap_or_else(|e| {
            tracing::warn!("读取别名注册表失败，已回退为空映射: {}", e);
            HashMap::new()
        })
    }

    /// 保存条目别名映射
    ///
    /// # 语义说明
    /// - 仅持久化到 `/etc/helmsman/aliases.json`，供界面展示；
    /// - **不会**修改 `/etc/default/grub`、`/etc/grub.d/*` 或 `grub.cfg`；
    /// - 传入空别名表示清除映射、回退原始标题。
    ///
    /// # Errors
    /// 当条目 ID 或显示名含非法字符、超长，或已有别名表损坏不可读时返回对应 [`DaemonError`]。
    pub fn set_alias(&self, entry_id: &str, alias: &str) -> Result<(), DaemonError> {
        validate_alias_binding(entry_id, alias)?;
        let _custom_guard = self.acquire_write_lock();

        let mut aliases = self.try_load_aliases()?;
        let trimmed_alias = alias.trim();

        if trimmed_alias.is_empty() {
            aliases.remove(entry_id);
        } else {
            aliases.insert(entry_id.to_string(), trimmed_alias.to_string());
        }

        if let Some(parent) = self.aliases_path.parent() {
            fs::create_dir_all(parent).map_err(|e| DaemonError::AtomicWriteFailed {
                reason: format!("创建别名目录 '{}' 失败: {}", parent.display(), e),
            })?;
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
            "成功更新条目显示别名（仅界面展示，不改写 grub.cfg），条目 ID: '{}' -> 别名: '{}'",
            entry_id, trimmed_alias
        );

        record_audit_event(&AuditEvent {
            caller_uid: resolve_caller_uid(),
            action: AuditAction::SetAlias,
            reason: format!("条目 '{}' 显示别名更新为 '{}'", entry_id, trimmed_alias),
            snapshot_id: None,
            success: true,
            duration: Duration::ZERO,
            diff_summary: None,
        });
        Ok(())
    }
}

/// 失败时将受管自定义脚本回滚至快照状态
fn rollback_custom_script(
    script_path: &std::path::Path,
    snapshot: Option<&grub_transaction_engine::SnapshotMeta>,
) {
    if let Some(snap) = snapshot {
        if let Err(restore_err) = restore_snapshot(snap) {
            tracing::error!(
                "自定义脚本回滚失败，路径: {}，原因: {}",
                script_path.display(),
                restore_err
            );
        } else {
            tracing::warn!(
                "后续步骤失败，自定义脚本已回滚至写入前状态，路径: {}",
                script_path.display()
            );
        }
    }
}

/// 校验别名绑定（条目 ID + 显示名）
///
/// # Errors
/// 当 ID 为空/超长/含控制字符，或显示名含控制字符/超长时返回 [`DaemonError::InvalidAlias`]。
fn validate_alias_binding(entry_id: &str, alias: &str) -> Result<(), DaemonError> {
    let id = entry_id.trim();
    if id.is_empty() || id.len() > 128 {
        return Err(DaemonError::InvalidAlias {
            entry_id: entry_id.to_string(),
            reason: "条目标识长度必须为 1-128 字符".to_string(),
        });
    }
    if id.chars().any(|c| c.is_control()) {
        return Err(DaemonError::InvalidAlias {
            entry_id: entry_id.to_string(),
            reason: "条目标识禁止包含控制字符".to_string(),
        });
    }

    let name = alias.trim();
    // 空别名表示清除，合法
    if name.is_empty() {
        return Ok(());
    }
    if name.len() > 128 {
        return Err(DaemonError::InvalidAlias {
            entry_id: entry_id.to_string(),
            reason: "显示别名不得超过 128 字符".to_string(),
        });
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(DaemonError::InvalidAlias {
            entry_id: entry_id.to_string(),
            reason: "显示别名禁止包含换行等控制字符".to_string(),
        });
    }
    Ok(())
}
