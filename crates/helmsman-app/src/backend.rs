use grub_boot_reader::{CustomBootEntry, MenuLayout};
use helmsman_client::{
    ApplyResultDto, CustomBootEntryDto, HelmsmanApiProxyBlocking, SnapshotDto, SystemStatusDto,
    ThemeInfoDto,
};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

/// 在线下载主题压缩包最大字节数限制 (64 MiB，与服务端限制一致)
const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;

/// 全局复用的阻塞式 HTTP 客户端单例（复用连接池与 TLS 上下文）
static HTTP_CLIENT: LazyLock<Result<reqwest::blocking::Client, String>> = LazyLock::new(|| {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("Helmsman/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败: {e}"))
});

/// 在线精选 GRUB 主题条目描述
#[derive(Debug, Clone, Copy)]
pub struct FeaturedOnlineTheme {
    /// 主题名称
    pub name: &'static str,
    /// 简短说明
    pub description: &'static str,
    /// 直链归档地址 (HTTPS)
    pub url: &'static str,
}

/// 内置精选开源 GRUB 主题列表
pub const FEATURED_THEMES: &[FeaturedOnlineTheme] = &[
    FeaturedOnlineTheme {
        name: "catppuccin-mocha",
        description: "Catppuccin Mocha 现代柔和深色主题 (GitHub 官方归档)",
        url: "https://github.com/catppuccin/grub/archive/refs/heads/main.zip",
    },
    FeaturedOnlineTheme {
        name: "fallout-grub-theme",
        description: "Fallout 复古终端风格 GRUB 主题 (GitHub 归档)",
        url: "https://github.com/shvchk/fallout-grub-theme/archive/refs/heads/master.zip",
    },
    FeaturedOnlineTheme {
        name: "poly-dark",
        description: "Poly Dark 极简高对比度深色主题 (GitHub 归档)",
        url: "https://github.com/shvchk/poly-dark/archive/refs/heads/master.zip",
    },
];

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
    /// 全量菜单编排布局
    pub menu_layout: MenuLayout,
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
            menu_layout: MenuLayout::default(),
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
    let menu_layout = proxy
        .get_menu_layout()
        .ok()
        .and_then(|raw| serde_json::from_str::<MenuLayout>(&raw).ok())
        .unwrap_or_default();
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
        menu_layout,
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

/// 提交全量菜单编排布局（排序、隐藏、子菜单归组）至特权守护进程
pub fn apply_menu_layout(layout: &MenuLayout, reason: &str) -> Result<ApplyResultDto, String> {
    let proxy = connect_proxy()?;
    let json = serde_json::to_string(layout).map_err(|e| format!("序列化菜单布局失败: {e}"))?;
    proxy
        .apply_menu_layout(&json, reason)
        .map_err(|e| format!("提交菜单布局失败: {e}"))
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

/// 刷新已安装主题列表
pub fn list_installed_themes() -> Result<Vec<ThemeInfoDto>, String> {
    let proxy = connect_proxy()?;
    proxy
        .list_themes()
        .map_err(|e| format!("获取已安装主题列表失败: {e}"))
}

/// 从本地压缩包路径安装 GRUB 主题
pub fn install_theme_archive(archive_path: &str, theme_name: &str) -> Result<String, String> {
    let proxy = connect_proxy()?;
    proxy
        .install_theme_archive(archive_path, theme_name)
        .map_err(|e| format!("安装主题失败: {e}"))
}

/// 卸载指定名称的已安装 GRUB 主题
pub fn remove_theme(theme_name: &str) -> Result<(), String> {
    let proxy = connect_proxy()?;
    proxy
        .remove_theme(theme_name)
        .map_err(|e| format!("卸载主题失败: {e}"))
}

/// 持久化引导项友好别名（空字符串表示清除别名）
pub fn set_entry_alias(entry_id: &str, alias: &str) -> Result<(), String> {
    let proxy = connect_proxy()?;
    proxy
        .set_entry_alias(entry_id, alias)
        .map_err(|e| format!("设置引导项别名失败: {e}"))
}

/// 临时下载文件 RAII 守卫，确保退出作用域时自动删除临时压缩包
struct TempFileGuard(PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// 流式下载在线主题归档（仅限 HTTPS 且受 64 MiB 体积上限约束）并安装至系统目录
pub fn download_and_install_theme(url: &str, theme_name: &str) -> Result<String, String> {
    let trimmed_url = url.trim();
    if !trimmed_url.starts_with("https://") {
        return Err("安全拦截：在线主题仅支持 https:// 加密下载链接".to_string());
    }

    let client = HTTP_CLIENT
        .as_ref()
        .map_err(|e| format!("网络客户端不可用: {e}"))?;

    info!("开始流式下载在线 GRUB 主题: {}", trimmed_url);
    let mut response = client
        .get(trimmed_url)
        .send()
        .map_err(|e| format!("请求主题压缩包失败: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "下载主题失败，HTTP 状态码: {}",
            response.status().as_u16()
        ));
    }

    if let Some(len) = response.content_length()
        && len > MAX_DOWNLOAD_BYTES
    {
        return Err(format!(
            "远端主题压缩包过大 ({len} 字节)，超过 64 MiB 安全上限"
        ));
    }

    let ext = if trimmed_url.ends_with(".tar.gz") || trimmed_url.ends_with(".tgz") {
        "tar.gz"
    } else if trimmed_url.ends_with(".tar") {
        "tar"
    } else {
        "zip"
    };
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_path = std::env::temp_dir().join(format!(
        "helmsman_theme_dl_{}_{}.{}",
        std::process::id(),
        ts,
        ext
    ));
    let _guard = TempFileGuard(temp_path.clone());

    let mut out = File::create(&temp_path).map_err(|e| format!("创建临时下载文件失败: {e}"))?;
    let mut buffer = [0u8; 64 * 1024];
    let mut total_bytes: u64 = 0;

    loop {
        let n = response
            .read(&mut buffer)
            .map_err(|e| format!("读取下载数据流失败: {e}"))?;
        if n == 0 {
            break;
        }
        total_bytes = total_bytes.saturating_add(n as u64);
        if total_bytes > MAX_DOWNLOAD_BYTES {
            return Err("下载数据流超出 64 MiB 安全上限，已中止".to_string());
        }
        out.write_all(&buffer[..n])
            .map_err(|e| format!("写入临时主题文件失败: {e}"))?;
    }
    out.flush()
        .map_err(|e| format!("刷新临时主题文件缓冲区失败: {e}"))?;
    drop(out);

    info!(
        "主题压缩包下载完成 ({} 字节)，正在交由特权服务安装...",
        total_bytes
    );
    install_theme_archive(&temp_path.to_string_lossy(), theme_name)
}
