use crate::audit::resolve_caller_uid_from_bus;
use crate::custom_manager::CustomManager;
use crate::idle::IdleWatcher;
use crate::polkit::check_polkit_authorization;
use crate::service::{GrubService, TransactionOptions};
use grub_boot_reader::CustomBootEntry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use zbus::DBusError;
use zbus::interface;
use zbus::message::Header;
use zvariant::Type;

/// D-Bus 服务名称
pub const DBUS_SERVICE_NAME: &str = "org.freedesktop.Helmsman";

/// D-Bus 对象挂载路径
pub const DBUS_OBJECT_PATH: &str = "/org/freedesktop/Helmsman";

/// D-Bus 领域契约接口版本 1
pub const DBUS_INTERFACE_V1: &str = "org.freedesktop.Helmsman.v1";

/// Polkit 权限动作定义（与 org.freedesktop.Helmsman.policy 一一对应）
pub mod polkit_actions {
    /// 读取系统引导状态与配置
    pub const ACTION_READ: &str = "org.freedesktop.Helmsman.read";
    /// 快速切换默认启动项（仅修改 grubenv）
    pub const ACTION_SET_DEFAULT: &str = "org.freedesktop.Helmsman.set-default";
    /// 提交配置变更并重新编译引导脚本
    pub const ACTION_APPLY_CHANGES: &str = "org.freedesktop.Helmsman.apply-changes";
    /// 回滚配置至指定历史快照
    pub const ACTION_ROLLBACK: &str = "org.freedesktop.Helmsman.rollback";
    /// 修改条目友好别名映射
    pub const ACTION_SET_ALIAS: &str = "org.freedesktop.Helmsman.set-alias";
    /// 安装 GRUB 主题压缩包
    pub const ACTION_INSTALL_THEME: &str = "org.freedesktop.Helmsman.install-theme";
}

/// Helmsman D-Bus 服务领域错误枚举
///
/// # 设计原理
/// - **实现初衷**：在 D-Bus 协议层向客户端返回具备明确领域命名空间与中文描述的强类型错误。
/// - **核心优势**：通过 zbus DBusError 宏自动映射为 D-Bus 标准错误报文，支持错误链与模式匹配。
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
    /// 业务执行失败
    Failed(String),
}

/// 系统引导状态 DTO
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SystemStatusDto {
    /// 操作系统发行版名称
    pub distro_name: String,
    /// 发行版家族
    pub distro_family: String,
    /// 固件类型 (UEFI 或 BIOS)
    pub firmware_type: String,
    /// 配置文件绝对路径
    pub config_path: String,
    /// 目标编译输出路径
    pub target_boot_path: String,
    /// 引导生成命令
    pub update_command: String,
}

/// 历史快照 DTO
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SnapshotDto {
    /// 快照唯一标识符
    pub id: String,
    /// 快照创建时间戳 (UNIX 秒)
    pub timestamp: u64,
    /// 创建原因说明
    pub reason: String,
    /// 目标文件绝对路径
    pub target_file: String,
    /// 备份副本所在路径
    pub backup_file: String,
}

/// 配置差异报告 DTO
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DiffResultDto {
    /// 是否存在实际修改
    pub has_changes: bool,
    /// 新增行数
    pub added_lines: usize,
    /// 删除行数
    pub removed_lines: usize,
    /// 统一差异文本
    pub diff_text: String,
}

/// 事务应用结果 DTO
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ApplyResultDto {
    /// 事务是否成功完成
    pub success: bool,
    /// 生成的快照 ID
    pub snapshot_id: String,
    /// 引导命令执行日志输出
    pub log_output: String,
    /// 失败时的错误信息（成功时为空字符串）
    pub error_message: String,
}

/// D-Bus 领域契约服务适配器
pub struct HelmsmanDbusAdapter {
    service: Arc<GrubService>,
    custom_manager: Arc<CustomManager>,
    idle_watcher: Arc<IdleWatcher>,
    options: TransactionOptions,
}

impl HelmsmanDbusAdapter {
    /// 使用现有服务实例创建适配器
    pub fn new(service: Arc<GrubService>) -> Self {
        Self {
            service,
            custom_manager: Arc::new(CustomManager::new_system_default()),
            idle_watcher: Arc::new(IdleWatcher::new()),
            options: TransactionOptions::default(),
        }
    }

    /// 使用指定的空闲观察器创建适配器（便于依赖注入与生命周期协同）
    pub fn with_idle_watcher(service: Arc<GrubService>, idle_watcher: Arc<IdleWatcher>) -> Self {
        Self {
            service,
            custom_manager: Arc::new(CustomManager::new_system_default()),
            idle_watcher,
            options: TransactionOptions::default(),
        }
    }

    /// 链式配置自定义引导项管理器（用于测试隔离）
    pub fn with_custom_manager(mut self, manager: Arc<CustomManager>) -> Self {
        self.custom_manager = manager;
        self
    }

    /// 链式配置事务执行选项（用于测试模拟或演练）
    pub fn with_options(mut self, options: TransactionOptions) -> Self {
        self.options = options;
        self
    }

    /// 获取关联的空闲观察器引用
    pub fn idle_watcher(&self) -> &Arc<IdleWatcher> {
        &self.idle_watcher
    }

    /// 校验 Polkit 权限，并在通过后解析调用方 Unix UID
    async fn verify_polkit(
        &self,
        header: Header<'_>,
        connection: &zbus::Connection,
        action_id: &str,
    ) -> Result<u32, HelmsmanDbusError> {
        let caller = match header.sender() {
            Some(s) => s.to_string(),
            None => "p2p-peer".to_string(),
        };

        let authorized = check_polkit_authorization(connection, &caller, action_id, true)
            .await
            .map_err(|e| HelmsmanDbusError::Failed(format!("PolicyKit 鉴权通信失败: {}", e)))?;

        if !authorized {
            return Err(HelmsmanDbusError::NotAuthorized(
                "未通过管理员身份验证".to_string(),
            ));
        }

        Ok(resolve_caller_uid_from_bus(connection, &caller).await)
    }

    /// 只读接口鉴权（allow_active 通常免密，但仍须走策略判定）
    async fn verify_polkit_read(
        &self,
        header: Header<'_>,
        connection: &zbus::Connection,
    ) -> Result<u32, HelmsmanDbusError> {
        let caller = match header.sender() {
            Some(s) => s.to_string(),
            None => "p2p-peer".to_string(),
        };

        let authorized =
            check_polkit_authorization(connection, &caller, polkit_actions::ACTION_READ, false)
                .await
                .map_err(|e| HelmsmanDbusError::Failed(format!("PolicyKit 鉴权通信失败: {}", e)))?;

        if !authorized {
            return Err(HelmsmanDbusError::NotAuthorized(
                "未通过引导配置读取身份验证".to_string(),
            ));
        }

        Ok(resolve_caller_uid_from_bus(connection, &caller).await)
    }
}

#[interface(
    name = "org.freedesktop.Helmsman.v1",
    proxy(
        default_service = "org.freedesktop.Helmsman",
        default_path = "/org/freedesktop/Helmsman"
    )
)]
impl HelmsmanDbusAdapter {
    /// 查询当前系统与引导适配器状态
    pub async fn get_system_status(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<SystemStatusDto, HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit_read(header, connection).await?;
        let profile = &self.service.distro_profile;
        Ok(SystemStatusDto {
            distro_name: profile.name.clone(),
            distro_family: format!("{:?}", profile.family),
            firmware_type: format!("{:?}", profile.firmware),
            config_path: self
                .service
                .default_config_path
                .to_string_lossy()
                .into_owned(),
            target_boot_path: profile.config_path.clone(),
            update_command: profile.update_command.clone(),
        })
    }

    /// 列出所有可用的历史配置快照
    pub async fn list_snapshots(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<Vec<SnapshotDto>, HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit_read(header, connection).await?;
        let snapshots = self
            .service
            .get_available_snapshots()
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))?;

        let mut dtos = Vec::with_capacity(snapshots.len());
        for s in snapshots {
            dtos.push(SnapshotDto {
                id: s.id,
                timestamp: s.timestamp,
                reason: s.reason,
                target_file: s.target_file.to_string_lossy().into_owned(),
                backup_file: s.backup_file.to_string_lossy().into_owned(),
            });
        }
        Ok(dtos)
    }

    /// 比对传入新配置与当前配置的差异
    pub async fn preview_changes(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        new_config: &str,
    ) -> Result<DiffResultDto, HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit_read(header, connection).await?;
        let report = self
            .service
            .preview_diff(new_config)
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))?;

        Ok(DiffResultDto {
            has_changes: report.has_changes,
            added_lines: report.added_lines,
            removed_lines: report.removed_lines,
            diff_text: report.diff_text,
        })
    }

    /// 通过 grubenv 快速切换默认启动项（受 Polkit set-default 权限保护）
    pub async fn set_default_entry(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        entry_id_or_title: &str,
    ) -> Result<(), HelmsmanDbusError> {
        self.idle_watcher.touch();
        if entry_id_or_title.trim().is_empty() {
            return Err(HelmsmanDbusError::InvalidArgs(
                "启动项标识或标题不能为空".to_string(),
            ));
        }

        let caller_uid = self
            .verify_polkit(header, connection, polkit_actions::ACTION_SET_DEFAULT)
            .await?;

        self.service
            .set_default_entry_fast_as(entry_id_or_title, caller_uid)
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))
    }

    /// 提交配置修改事务（受 Polkit apply-changes 权限保护）
    pub async fn apply_changes(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        new_config: &str,
        reason: &str,
    ) -> Result<ApplyResultDto, HelmsmanDbusError> {
        let _guard = self.idle_watcher.enter_busy();
        if new_config.trim().is_empty() {
            return Err(HelmsmanDbusError::InvalidArgs(
                "提交的配置内容不能为空".to_string(),
            ));
        }

        let caller_uid = self
            .verify_polkit(header, connection, polkit_actions::ACTION_APPLY_CHANGES)
            .await?;

        let result = self
            .service
            .apply_changes_as(new_config, reason, &self.options, caller_uid)
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))?;

        Ok(ApplyResultDto {
            success: result.success,
            snapshot_id: result.snapshot_id,
            log_output: result.log_output,
            error_message: result.error_message.unwrap_or_default(),
        })
    }

    /// 回滚至指定历史快照（受 Polkit rollback 权限保护）
    pub async fn rollback_snapshot(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        snapshot_id: &str,
    ) -> Result<(), HelmsmanDbusError> {
        let _guard = self.idle_watcher.enter_busy();
        if snapshot_id.trim().is_empty() {
            return Err(HelmsmanDbusError::InvalidArgs(
                "快照 ID 不能为空".to_string(),
            ));
        }

        let caller_uid = self
            .verify_polkit(header, connection, polkit_actions::ACTION_ROLLBACK)
            .await?;

        self.service
            .rollback_to_snapshot_as(snapshot_id, caller_uid)
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))
    }

    /// 读取受管的自定义引导项列表 (/etc/grub.d/41_helmsman_custom)
    pub async fn get_custom_entries(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<Vec<CustomBootEntry>, HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit_read(header, connection).await?;
        self.custom_manager
            .load_custom_entries()
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))
    }

    /// 提交自定义引导项修改（受 Polkit apply-changes 权限保护）
    pub async fn apply_custom_entries(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        entries: Vec<CustomBootEntry>,
        reason: &str,
    ) -> Result<ApplyResultDto, HelmsmanDbusError> {
        let _guard = self.idle_watcher.enter_busy();
        let caller_uid = self
            .verify_polkit(header, connection, polkit_actions::ACTION_APPLY_CHANGES)
            .await?;

        let result = self
            .custom_manager
            .save_custom_entries_as(&self.service, &entries, reason, &self.options, caller_uid)
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))?;

        Ok(ApplyResultDto {
            success: result.success,
            snapshot_id: result.snapshot_id,
            log_output: result.log_output,
            error_message: result.error_message.unwrap_or_default(),
        })
    }

    /// 获取所有条目别名映射表
    pub async fn get_entry_aliases(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<HashMap<String, String>, HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit_read(header, connection).await?;
        Ok(self.custom_manager.load_aliases())
    }

    /// 设置条目别名映射（受 Polkit set-alias 权限保护）
    pub async fn set_entry_alias(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        entry_id: &str,
        alias: &str,
    ) -> Result<(), HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit(header, connection, polkit_actions::ACTION_SET_ALIAS)
            .await?;

        self.custom_manager
            .set_alias(entry_id, alias)
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))
    }

    /// 安装主题压缩包至系统主题目录（受 Polkit install-theme 权限保护）
    pub async fn install_theme_archive(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        archive_path: &str,
        theme_name: &str,
    ) -> Result<String, HelmsmanDbusError> {
        let _guard = self.idle_watcher.enter_busy();
        if archive_path.trim().is_empty() {
            return Err(HelmsmanDbusError::InvalidArgs(
                "主题压缩包路径不能为空".to_string(),
            ));
        }

        let caller_uid = self
            .verify_polkit(header, connection, polkit_actions::ACTION_INSTALL_THEME)
            .await?;

        let opt_name = if theme_name.trim().is_empty() {
            None
        } else {
            Some(theme_name.trim())
        };

        let installed_path = self
            .service
            .install_theme_archive_as(std::path::Path::new(archive_path), opt_name, caller_uid)
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))?;

        Ok(installed_path.to_string_lossy().into_owned())
    }
}

impl fmt::Display for SystemStatusDto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}, {}) - 配置文件: {}",
            self.distro_name, self.distro_family, self.firmware_type, self.config_path
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grub_distro_adapter::{DistroFamily, DistroProfile, FirmwareType};
    use std::fs;
    use std::path::PathBuf;

    fn get_dbus_test_dir(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join("grub_dbus_test").join(name);
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        let config_file = base.join("default_grub");
        let backup_dir = base.join("backups");
        fs::create_dir_all(&backup_dir).unwrap();
        (config_file, backup_dir)
    }

    #[test]
    fn test_dbus_constants_and_contract() {
        assert_eq!(DBUS_SERVICE_NAME, "org.freedesktop.Helmsman");
        assert_eq!(DBUS_OBJECT_PATH, "/org/freedesktop/Helmsman");
        assert_eq!(DBUS_INTERFACE_V1, "org.freedesktop.Helmsman.v1");
        assert_eq!(polkit_actions::ACTION_READ, "org.freedesktop.Helmsman.read");
        assert_eq!(
            polkit_actions::ACTION_SET_DEFAULT,
            "org.freedesktop.Helmsman.set-default"
        );
        assert_eq!(
            polkit_actions::ACTION_APPLY_CHANGES,
            "org.freedesktop.Helmsman.apply-changes"
        );
        assert_eq!(
            polkit_actions::ACTION_ROLLBACK,
            "org.freedesktop.Helmsman.rollback"
        );
        assert_eq!(
            polkit_actions::ACTION_SET_ALIAS,
            "org.freedesktop.Helmsman.set-alias"
        );
        assert_eq!(
            polkit_actions::ACTION_INSTALL_THEME,
            "org.freedesktop.Helmsman.install-theme"
        );
    }

    #[tokio::test]
    async fn test_dbus_adapter_idle_watcher_integration() {
        let (config_file, backup_dir) = get_dbus_test_dir("idle_integration");
        fs::write(&config_file, "GRUB_DEFAULT=0\n").unwrap();

        let distro_profile = DistroProfile {
            family: DistroFamily::DebianUbuntu,
            name: "Ubuntu Linux".to_string(),
            firmware: FirmwareType::Uefi,
            config_path: "/boot/grub/grub.cfg".to_string(),
            update_command: "/usr/sbin/update-grub".to_string(),
            command_args: Vec::new(),
            check_command: "/usr/bin/grub-script-check".to_string(),
            check_command_args: Vec::new(),
            grubenv_path: "/boot/grub/grubenv".to_string(),
            set_default_command: "/usr/bin/grub-set-default".to_string(),
        };

        let service = Arc::new(GrubService::new_with_paths(
            config_file,
            backup_dir,
            distro_profile,
        ));
        let watcher = Arc::new(IdleWatcher::new());
        let adapter = HelmsmanDbusAdapter::with_idle_watcher(service, Arc::clone(&watcher));

        // 模拟 120 秒前活跃
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        watcher.set_last_active_for_test(now - 120);
        assert!(watcher.is_idle_timeout(60));

        // 通过业务层预览（不经过 D-Bus 接口鉴权）验证服务仍可用；idle touch 由接口层负责
        let preview = adapter.idle_watcher().is_idle_timeout(60);
        assert!(preview);

        watcher.set_last_active_for_test(now);
        assert!(!watcher.is_idle_timeout(60));
        assert!(Arc::ptr_eq(adapter.idle_watcher(), &watcher));
    }
}
