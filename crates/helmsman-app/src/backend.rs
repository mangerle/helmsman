use grub_boot_reader::CustomBootEntry;
use helmsman_client::{
    ApplyResultDto, CustomBootEntryDto, HelmsmanApiProxyBlocking, SnapshotDto, SystemStatusDto,
    ThemeInfoDto,
};
use std::collections::HashMap;
use tracing::{info, warn};

/// 从特权守护进程拉取的全量初始快照（无法连接总线时提供安全演示模式）
#[derive(Debug, Clone)]
pub struct BackendSnapshot {
    /// 系统与适配器状态（演示模式下为 None）
    pub system_status: Option<SystemStatusDto>,
    /// `/etc/default/grub` 原文
    pub default_config: String,
    /// `/boot/grub/grub.cfg` 原文
    pub grub_cfg: String,
    /// 历史快照列表
    pub snapshots: Vec<SnapshotDto>,
    /// 已安装主题列表
    pub themes: Vec<ThemeInfoDto>,
    /// 受管自定义引导项
    pub custom_entries: Vec<CustomBootEntry>,
    /// 条目友好别名映射
    pub aliases: HashMap<String, String>,
    /// 是否处于未连接守护进程的离线演示模式
    pub is_demo_mode: bool,
    /// 顶部连接状态说明
    pub status_note: String,
}

impl BackendSnapshot {
    /// 尝试连接 D-Bus 系统总线加载真实配置；不可用时回退至本地只读演示环境
    pub fn load() -> Self {
        match try_load_from_dbus() {
            Ok(snapshot) => {
                info!("已连接 helmsman-daemon 系统服务并加载引导配置");
                snapshot
            }
            Err(err) => {
                warn!("未能连接 helmsman-daemon，已切换为只读演示模式: {err}");
                Self::demo_fallback(&err)
            }
        }
    }

    /// 本地演示模式基准数据（仅内存预览，不触碰真实系统文件）
    pub fn demo_fallback(reason: &str) -> Self {
        let default_config = r#"# GRUB 引导配置文件（演示模式）
GRUB_DEFAULT=0
GRUB_TIMEOUT_STYLE=menu
GRUB_TIMEOUT=5
GRUB_DISTRIBUTOR="Helmsman Linux"
GRUB_CMDLINE_LINUX_DEFAULT="quiet splash"
GRUB_CMDLINE_LINUX=""
GRUB_GFXMODE=1920x1080
"#
        .to_string();

        let grub_cfg = r#"
menuentry 'Ubuntu 24.04 LTS' --class ubuntu --class gnu-linux --id 'gnulinux-simple-ubuntu' {
    linux /boot/vmlinuz-6.8.0-generic root=UUID=1122-3344 ro quiet splash
    initrd /boot/initrd.img-6.8.0-generic
}
submenu 'Advanced options for Ubuntu' --id 'gnulinux-advanced' {
    menuentry 'Ubuntu, with Linux 6.8.0-generic' --id 'gnulinux-6.8.0-advanced' {
        linux /boot/vmlinuz-6.8.0-generic root=UUID=1122-3344 ro quiet splash
        initrd /boot/initrd.img-6.8.0-generic
    }
    menuentry 'Ubuntu, with Linux 6.8.0-generic (recovery mode)' --id 'gnulinux-6.8.0-recovery' {
        linux /boot/vmlinuz-6.8.0-generic root=UUID=1122-3344 ro recovery nomodeset
        initrd /boot/initrd.img-6.8.0-generic
    }
}
menuentry 'Windows Boot Manager (on /dev/nvme0n1p1)' --class windows --id 'osprober-efi-win' {
    chainloader /EFI/Microsoft/Boot/bootmgfw.efi
}
"#
        .to_string();

        Self {
            system_status: None,
            default_config,
            grub_cfg,
            snapshots: Vec::new(),
            themes: Vec::new(),
            custom_entries: Vec::new(),
            aliases: HashMap::new(),
            is_demo_mode: true,
            status_note: format!("演示模式（未连接守护进程: {reason}）"),
        }
    }
}

fn connect_proxy() -> Result<HelmsmanApiProxyBlocking<'static>, String> {
    let conn = zbus::blocking::Connection::system()
        .map_err(|e| format!("连接 D-Bus 系统总线失败: {e}"))?;
    HelmsmanApiProxyBlocking::new(&conn).map_err(|e| format!("创建 D-Bus 客户端代理失败: {e}"))
}

fn try_load_from_dbus() -> Result<BackendSnapshot, String> {
    let proxy = connect_proxy()?;
    let system_status = proxy
        .get_system_status()
        .map_err(|e| format!("获取系统状态失败: {e}"))?;
    let default_config = proxy
        .get_current_config()
        .map_err(|e| format!("读取当前配置失败: {e}"))?;
    let grub_cfg = proxy
        .get_boot_cfg()
        .map_err(|e| format!("读取引导菜单失败: {e}"))?;
    let snapshots = proxy.list_snapshots().unwrap_or_default();
    let themes = proxy.list_themes().unwrap_or_default();
    let custom_entries = proxy
        .get_custom_entries()
        .unwrap_or_default()
        .into_iter()
        .map(CustomBootEntry::from)
        .collect();
    let aliases = proxy.get_entry_aliases().unwrap_or_default();
    let status_note = format!(
        "已连接 · {} ({:?})",
        system_status.distro_name, system_status.firmware_type
    );

    Ok(BackendSnapshot {
        system_status: Some(system_status),
        default_config,
        grub_cfg,
        snapshots,
        themes,
        custom_entries,
        aliases,
        is_demo_mode: false,
        status_note,
    })
}

/// 提交配置修改至特权守护进程
pub fn apply_config_changes(new_config: &str, reason: &str) -> Result<ApplyResultDto, String> {
    let proxy = connect_proxy()?;
    proxy
        .apply_changes(new_config, reason)
        .map_err(|e| format!("提交配置变更失败: {e}"))
}

/// 提交自定义引导项列表至特权守护进程
pub fn apply_custom_entries(
    entries: &[CustomBootEntry],
    reason: &str,
) -> Result<ApplyResultDto, String> {
    let proxy = connect_proxy()?;
    let dtos: Vec<CustomBootEntryDto> = entries.iter().cloned().map(Into::into).collect();
    proxy
        .apply_custom_entries(dtos, reason)
        .map_err(|e| format!("提交自定义引导项失败: {e}"))
}

/// 回滚至指定历史快照
pub fn rollback_snapshot(snapshot_id: &str) -> Result<(), String> {
    let proxy = connect_proxy()?;
    proxy
        .rollback_snapshot(snapshot_id)
        .map_err(|e| format!("回滚快照失败: {e}"))
}

/// 删除指定历史快照
pub fn delete_snapshot(snapshot_id: &str) -> Result<(), String> {
    let proxy = connect_proxy()?;
    proxy
        .delete_snapshot(snapshot_id)
        .map_err(|e| format!("删除快照失败: {e}"))
}
