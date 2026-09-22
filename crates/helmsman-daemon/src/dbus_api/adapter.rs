use crate::audit::resolve_caller_uid_from_bus;
use crate::custom_manager::CustomManager;
use crate::dbus_api::error::HelmsmanDbusError;
use crate::dbus_api::{
    ApplyResultDto, DiffResultDto, SnapshotDto, SystemStatusDto, polkit_actions,
};
use crate::idle::IdleWatcher;
use crate::polkit::check_polkit_authorization;
use crate::service::{GrubService, TransactionOptions};
use grub_boot_reader::CustomBootEntry;
use std::collections::HashMap;
use std::sync::Arc;
use zbus::interface;
use zbus::message::Header;

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

    /// 读取当前 /etc/default/grub 原文
    pub async fn get_current_config(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<String, HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit_read(header, connection).await?;
        let service = Arc::clone(&self.service);
        tokio::task::spawn_blocking(move || {
            std::fs::read_to_string(&service.default_config_path)
                .map_err(|e| HelmsmanDbusError::Failed(format!("读取当前配置失败: {e}")))
        })
        .await
        .map_err(|e| HelmsmanDbusError::Failed(format!("配置读取任务异常终止: {e}")))?
    }

    /// 查询服务版本号
    pub async fn get_version(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<String, HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit_read(header, connection).await?;
        Ok(env!("CARGO_PKG_VERSION").to_string())
    }

    /// 列出所有可用的历史配置快照
    pub async fn list_snapshots(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<Vec<SnapshotDto>, HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit_read(header, connection).await?;
        let service = Arc::clone(&self.service);
        let snapshots = tokio::task::spawn_blocking(move || service.get_available_snapshots())
            .await
            .map_err(|e| HelmsmanDbusError::Failed(format!("快照列表任务异常终止: {e}")))?
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
        let service = Arc::clone(&self.service);
        let new_config = new_config.to_string();
        let report = tokio::task::spawn_blocking(move || service.preview_diff(&new_config))
            .await
            .map_err(|e| HelmsmanDbusError::Failed(format!("差异预览任务异常终止: {e}")))?
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

        let service = Arc::clone(&self.service);
        let entry_id_or_title = entry_id_or_title.to_string();
        tokio::task::spawn_blocking(move || {
            service.set_default_entry_fast_as(&entry_id_or_title, caller_uid)
        })
        .await
        .map_err(|e| HelmsmanDbusError::Failed(format!("默认项切换任务异常终止: {e}")))?
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

        let service = Arc::clone(&self.service);
        let options = self.options.clone();
        let new_config = new_config.to_string();
        let reason = reason.to_string();
        let result = tokio::task::spawn_blocking(move || {
            service.apply_changes_as(&new_config, &reason, &options, caller_uid)
        })
        .await
        .map_err(|e| HelmsmanDbusError::Failed(format!("配置提交任务异常终止: {e}")))?
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

        let service = Arc::clone(&self.service);
        let snapshot_id = snapshot_id.to_string();
        tokio::task::spawn_blocking(move || {
            service.rollback_to_snapshot_as(&snapshot_id, caller_uid)
        })
        .await
        .map_err(|e| HelmsmanDbusError::Failed(format!("快照回滚任务异常终止: {e}")))?
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
        let custom_manager = Arc::clone(&self.custom_manager);
        tokio::task::spawn_blocking(move || custom_manager.load_custom_entries())
            .await
            .map_err(|e| HelmsmanDbusError::Failed(format!("自定义条目读取任务异常终止: {e}")))?
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

        let custom_manager = Arc::clone(&self.custom_manager);
        let service = Arc::clone(&self.service);
        let options = self.options.clone();
        let reason = reason.to_string();
        let result = tokio::task::spawn_blocking(move || {
            custom_manager.save_custom_entries_as(&service, &entries, &reason, &options, caller_uid)
        })
        .await
        .map_err(|e| HelmsmanDbusError::Failed(format!("自定义条目提交任务异常终止: {e}")))?
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
        let custom_manager = Arc::clone(&self.custom_manager);
        let aliases = tokio::task::spawn_blocking(move || custom_manager.load_aliases())
            .await
            .map_err(|e| HelmsmanDbusError::Failed(format!("别名读取任务异常终止: {e}")))?;
        Ok(aliases)
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

        let custom_manager = Arc::clone(&self.custom_manager);
        let entry_id = entry_id.to_string();
        let alias = alias.to_string();
        tokio::task::spawn_blocking(move || custom_manager.set_alias(&entry_id, &alias))
            .await
            .map_err(|e| HelmsmanDbusError::Failed(format!("别名写入任务异常终止: {e}")))?
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
            Some(theme_name.trim().to_string())
        };

        let service = Arc::clone(&self.service);
        let archive_path = archive_path.to_string();
        let installed_path = tokio::task::spawn_blocking(move || {
            service.install_theme_archive_as(
                std::path::Path::new(&archive_path),
                opt_name.as_deref(),
                caller_uid,
            )
        })
        .await
        .map_err(|e| HelmsmanDbusError::Failed(format!("主题安装任务异常终止: {e}")))?
        .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))?;

        Ok(installed_path.to_string_lossy().into_owned())
    }

    /// 列出已安装主题
    pub async fn list_themes(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<Vec<crate::dbus_api::ThemeInfoDto>, HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit_read(header, connection).await?;
        let service = Arc::clone(&self.service);
        let themes = tokio::task::spawn_blocking(move || service.list_themes())
            .await
            .map_err(|e| HelmsmanDbusError::Failed(format!("主题列表任务异常终止: {e}")))?
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))?;

        Ok(themes
            .into_iter()
            .map(|t| crate::dbus_api::ThemeInfoDto {
                name: t.name,
                path: t.path.to_string_lossy().into_owned(),
                has_descriptor: t.has_descriptor,
            })
            .collect())
    }

    /// 卸载指定主题（受 Polkit install-theme 权限保护）
    pub async fn remove_theme(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        theme_name: &str,
    ) -> Result<(), HelmsmanDbusError> {
        let _guard = self.idle_watcher.enter_busy();
        if theme_name.trim().is_empty() {
            return Err(HelmsmanDbusError::InvalidArgs(
                "主题名称不能为空".to_string(),
            ));
        }

        let caller_uid = self
            .verify_polkit(header, connection, polkit_actions::ACTION_INSTALL_THEME)
            .await?;

        let service = Arc::clone(&self.service);
        let theme_name = theme_name.trim().to_string();
        tokio::task::spawn_blocking(move || service.remove_theme_as(&theme_name, caller_uid))
            .await
            .map_err(|e| HelmsmanDbusError::Failed(format!("主题卸载任务异常终止: {e}")))?
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))
    }

    /// 删除指定快照（受 Polkit rollback 权限保护）
    pub async fn delete_snapshot(
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

        let service = Arc::clone(&self.service);
        let snapshot_id = snapshot_id.trim().to_string();
        tokio::task::spawn_blocking(move || service.delete_snapshot_as(&snapshot_id, caller_uid))
            .await
            .map_err(|e| HelmsmanDbusError::Failed(format!("快照删除任务异常终止: {e}")))?
            .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))
    }

    /// 导出指定快照（受 Polkit read 权限保护）
    pub async fn export_snapshot(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        snapshot_id: &str,
        dest_path: &str,
    ) -> Result<String, HelmsmanDbusError> {
        let _guard = self.idle_watcher.enter_busy();
        if snapshot_id.trim().is_empty() || dest_path.trim().is_empty() {
            return Err(HelmsmanDbusError::InvalidArgs(
                "快照 ID 与导出路径均不能为空".to_string(),
            ));
        }

        let caller_uid = self
            .verify_polkit(header, connection, polkit_actions::ACTION_READ)
            .await?;

        let service = Arc::clone(&self.service);
        let snapshot_id = snapshot_id.trim().to_string();
        let dest_path = dest_path.trim().to_string();
        let exported = tokio::task::spawn_blocking(move || {
            service.export_snapshot_as(&snapshot_id, std::path::Path::new(&dest_path), caller_uid)
        })
        .await
        .map_err(|e| HelmsmanDbusError::Failed(format!("快照导出任务异常终止: {e}")))?
        .map_err(|e| HelmsmanDbusError::Failed(e.to_string()))?;

        Ok(exported.to_string_lossy().into_owned())
    }
}
