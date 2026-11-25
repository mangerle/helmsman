use crate::audit::resolve_caller_uid_from_bus;
use crate::custom_manager::CustomManager;
use crate::dbus_api::error::HelmsmanDbusError;
use crate::dbus_api::{
    ApplyResultDto, DiffResultDto, SnapshotDto, SystemStatusDto, ThemeInfoDto, polkit_actions,
};
use crate::idle::IdleWatcher;
use crate::polkit::check_polkit_authorization;
use crate::service::{DaemonError, GrubService, TransactionOptions, TransactionResult};
use grub_boot_reader::CustomBootEntry;
use grub_transaction_engine::DiffReport;
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
        Self::with_idle_watcher(service, Arc::new(IdleWatcher::new()))
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

    /// 统一的 Polkit 权限校验与调用方 UID 解析入口
    async fn authorize(
        &self,
        header: Header<'_>,
        connection: &zbus::Connection,
        action_id: &str,
        allow_interaction: bool,
    ) -> Result<u32, HelmsmanDbusError> {
        let caller = header
            .sender()
            .map_or_else(|| "p2p-peer".to_string(), |s| s.to_string());

        let authorized =
            check_polkit_authorization(connection, &caller, action_id, allow_interaction)
                .await
                .map_err(|e| {
                    HelmsmanDbusError::Failed(format!(
                        "PolicyKit 鉴权通信失败，操作: {action_id}，原因: {e}"
                    ))
                })?;

        if !authorized {
            return Err(HelmsmanDbusError::NotAuthorized(format!(
                "未通过身份验证，操作: {action_id}"
            )));
        }

        Ok(resolve_caller_uid_from_bus(connection, &caller).await)
    }

    /// 便捷方法：只读权限校验（allow_active 通常免密，但仍走 Polkit 策略）
    async fn verify_polkit_read(
        &self,
        header: Header<'_>,
        connection: &zbus::Connection,
    ) -> Result<u32, HelmsmanDbusError> {
        self.authorize(header, connection, polkit_actions::ACTION_READ, false)
            .await
    }

    /// 便捷方法：交互式管理员权限校验
    async fn verify_polkit(
        &self,
        header: Header<'_>,
        connection: &zbus::Connection,
        action_id: &str,
    ) -> Result<u32, HelmsmanDbusError> {
        self.authorize(header, connection, action_id, true).await
    }
}

/// 在阻塞线程池中执行同步服务调用并统一映射任务中止与领域错误
async fn spawn_service_task<T, E, F>(task_name: &str, op: F) -> Result<T, HelmsmanDbusError>
where
    T: Send + 'static,
    E: Into<HelmsmanDbusError> + Send + 'static,
    F: FnOnce() -> Result<T, E> + Send + 'static,
{
    tokio::task::spawn_blocking(op)
        .await
        .map_err(|e| HelmsmanDbusError::Failed(format!("{task_name}异常终止: {e}")))?
        .map_err(Into::into)
}

/// 校验非空字符串参数并返回去空白后的独立 `String`
fn require_non_empty(value: &str, field_label: &str) -> Result<String, HelmsmanDbusError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(HelmsmanDbusError::InvalidArgs(format!(
            "{field_label}不能为空"
        )));
    }
    Ok(trimmed.to_string())
}

fn to_apply_dto(result: TransactionResult) -> ApplyResultDto {
    ApplyResultDto {
        success: result.success,
        snapshot_id: result.snapshot_id,
        log_output: result.log_output,
        error_message: result.error_message.unwrap_or_default(),
    }
}

fn to_diff_dto(report: DiffReport) -> DiffResultDto {
    DiffResultDto {
        has_changes: report.has_changes,
        added_lines: report.added_lines,
        removed_lines: report.removed_lines,
        diff_text: report.diff_text,
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
        let path = self.service.default_config_path.clone();
        spawn_service_task("配置读取任务", move || {
            std::fs::read_to_string(&path).map_err(|e| DaemonError::ConfigReadFailed {
                path,
                reason: e.to_string(),
            })
        })
        .await
    }

    /// 读取编译后的引导菜单（grub.cfg）原文
    ///
    /// # 设计原理
    /// 菜单路径由 Distro Adapter 认定，集中在此读出，前端零文件访问。
    pub async fn get_boot_cfg(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<String, HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit_read(header, connection).await?;
        let boot_cfg_path = self.service.distro_profile.config_path.clone();
        spawn_service_task("引导菜单读取任务", move || {
            std::fs::read_to_string(&boot_cfg_path).map_err(|e| {
                HelmsmanDbusError::Failed(format!(
                    "读取引导菜单失败，路径: {boot_cfg_path}，原因: {e}"
                ))
            })
        })
        .await
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
        let snapshots = spawn_service_task("快照列表任务", move || {
            service.get_available_snapshots()
        })
        .await?;

        Ok(snapshots
            .into_iter()
            .map(|s| SnapshotDto {
                id: s.id,
                timestamp: s.timestamp,
                reason: s.reason,
                target_file: s.target_file.to_string_lossy().into_owned(),
                backup_file: s.backup_file.to_string_lossy().into_owned(),
            })
            .collect())
    }

    /// 预览指定快照与当前配置的差异（受 Polkit read 权限保护）
    pub async fn preview_snapshot_changes(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        snapshot_id: &str,
    ) -> Result<DiffResultDto, HelmsmanDbusError> {
        self.idle_watcher.touch();
        let snapshot_id = require_non_empty(snapshot_id, "快照 ID ")?;
        self.verify_polkit_read(header, connection).await?;
        let service = Arc::clone(&self.service);
        let report = spawn_service_task("快照差异预览任务", move || {
            service.preview_snapshot_diff(&snapshot_id)
        })
        .await?;
        Ok(to_diff_dto(report))
    }

    /// 读取指定快照的备份内容原文（受 Polkit read 权限保护）
    pub async fn get_snapshot_content(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        snapshot_id: &str,
    ) -> Result<String, HelmsmanDbusError> {
        self.idle_watcher.touch();
        let snapshot_id = require_non_empty(snapshot_id, "快照 ID ")?;
        self.verify_polkit_read(header, connection).await?;
        let service = Arc::clone(&self.service);
        spawn_service_task("快照内容读取任务", move || {
            service.get_snapshot_content(&snapshot_id)
        })
        .await
    }

    /// 按保留上限有界裁剪历史快照（受 Polkit rollback 权限保护）
    pub async fn prune_snapshots(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        max_keep: u32,
    ) -> Result<u32, HelmsmanDbusError> {
        let _guard = self.idle_watcher.enter_busy();
        let _caller_uid = self
            .verify_polkit(header, connection, polkit_actions::ACTION_ROLLBACK)
            .await?;
        let service = Arc::clone(&self.service);
        let removed = spawn_service_task("快照裁剪任务", move || {
            service.prune_snapshot_history(max_keep as usize)
        })
        .await?;
        Ok(removed as u32)
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
        let report = spawn_service_task("差异预览任务", move || {
            service.preview_diff(&new_config)
        })
        .await?;
        Ok(to_diff_dto(report))
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
        spawn_service_task("默认项切换任务", move || {
            service.set_default_entry_fast_as(&entry_id_or_title, caller_uid)
        })
        .await
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
        let result = spawn_service_task("配置提交任务", move || {
            service.apply_changes_as(&new_config, &reason, &options, caller_uid)
        })
        .await?;
        Ok(to_apply_dto(result))
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
        spawn_service_task("快照回滚任务", move || {
            service.rollback_to_snapshot_as(&snapshot_id, caller_uid)
        })
        .await
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
        spawn_service_task("自定义条目读取任务", move || {
            custom_manager.load_custom_entries()
        })
        .await
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
        let result = spawn_service_task("自定义条目提交任务", move || {
            custom_manager.save_custom_entries_as(&service, &entries, &reason, &options, caller_uid)
        })
        .await?;
        Ok(to_apply_dto(result))
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
        spawn_service_task("别名读取任务", move || {
            Ok::<_, HelmsmanDbusError>(custom_manager.load_aliases())
        })
        .await
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
        spawn_service_task("别名写入任务", move || {
            custom_manager.set_alias(&entry_id, &alias)
        })
        .await
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
        let opt_name = (!theme_name.trim().is_empty()).then(|| theme_name.trim().to_string());
        let service = Arc::clone(&self.service);
        let archive_path = archive_path.to_string();
        let installed_path = spawn_service_task("主题安装任务", move || {
            service.install_theme_archive_as(
                std::path::Path::new(&archive_path),
                opt_name.as_deref(),
                caller_uid,
            )
        })
        .await?;
        Ok(installed_path.to_string_lossy().into_owned())
    }

    /// 列出已安装主题
    pub async fn list_themes(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> Result<Vec<ThemeInfoDto>, HelmsmanDbusError> {
        self.idle_watcher.touch();
        self.verify_polkit_read(header, connection).await?;
        let service = Arc::clone(&self.service);
        let themes = spawn_service_task("主题列表任务", move || service.list_themes()).await?;
        Ok(themes
            .into_iter()
            .map(|t| ThemeInfoDto {
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
        let theme_name = require_non_empty(theme_name, "主题名称")?;
        let caller_uid = self
            .verify_polkit(header, connection, polkit_actions::ACTION_INSTALL_THEME)
            .await?;
        let service = Arc::clone(&self.service);
        spawn_service_task("主题卸载任务", move || {
            service.remove_theme_as(&theme_name, caller_uid)
        })
        .await
    }

    /// 删除指定快照（受 Polkit rollback 权限保护）
    pub async fn delete_snapshot(
        &self,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &zbus::Connection,
        snapshot_id: &str,
    ) -> Result<(), HelmsmanDbusError> {
        let _guard = self.idle_watcher.enter_busy();
        let snapshot_id = require_non_empty(snapshot_id, "快照 ID ")?;
        let caller_uid = self
            .verify_polkit(header, connection, polkit_actions::ACTION_ROLLBACK)
            .await?;
        let service = Arc::clone(&self.service);
        spawn_service_task("快照删除任务", move || {
            service.delete_snapshot_as(&snapshot_id, caller_uid)
        })
        .await
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
        let exported = spawn_service_task("快照导出任务", move || {
            service.export_snapshot_as(&snapshot_id, std::path::Path::new(&dest_path), caller_uid)
        })
        .await?;
        Ok(exported.to_string_lossy().into_owned())
    }
}
