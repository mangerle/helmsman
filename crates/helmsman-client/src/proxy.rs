use crate::types::{ApplyResultDto, DiffResultDto, SnapshotDto, SystemStatusDto};
use grub_boot_reader::CustomBootEntry;
use std::collections::HashMap;

/// Helmsman 特权服务 D-Bus 客户端契约
///
/// # 设计原理
/// - **实现初衷**：非特权 UI / 脚本通过系统总线调用守护进程，不在前端链接特权业务实现。
/// - **代价与局限**：调用会触发 Polkit 鉴权；失败以 D-Bus 错误名返回。
#[zbus::proxy(
    interface = "org.freedesktop.Helmsman.v1",
    default_service = "org.freedesktop.Helmsman",
    default_path = "/org/freedesktop/Helmsman",
    gen_blocking = true
)]
pub trait HelmsmanApi {
    /// 查询当前系统与引导适配器状态
    fn get_system_status(&self) -> zbus::Result<SystemStatusDto>;

    /// 列出所有可用的历史配置快照
    fn list_snapshots(&self) -> zbus::Result<Vec<SnapshotDto>>;

    /// 比对传入新配置与当前配置的差异
    fn preview_changes(&self, new_config: &str) -> zbus::Result<DiffResultDto>;

    /// 快速切换默认启动项
    fn set_default_entry(&self, entry_id_or_title: &str) -> zbus::Result<()>;

    /// 提交配置修改事务
    fn apply_changes(&self, new_config: &str, reason: &str) -> zbus::Result<ApplyResultDto>;

    /// 回滚至指定历史快照
    fn rollback_snapshot(&self, snapshot_id: &str) -> zbus::Result<()>;

    /// 读取受管自定义引导项
    fn get_custom_entries(&self) -> zbus::Result<Vec<CustomBootEntry>>;

    /// 提交自定义引导项修改
    fn apply_custom_entries(
        &self,
        entries: Vec<CustomBootEntry>,
        reason: &str,
    ) -> zbus::Result<ApplyResultDto>;

    /// 获取条目别名映射
    fn get_entry_aliases(&self) -> zbus::Result<HashMap<String, String>>;

    /// 设置条目别名
    fn set_entry_alias(&self, entry_id: &str, alias: &str) -> zbus::Result<()>;

    /// 安装主题压缩包
    fn install_theme_archive(&self, archive_path: &str, theme_name: &str) -> zbus::Result<String>;
}
