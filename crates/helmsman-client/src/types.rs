/// D-Bus 服务名称
pub const DBUS_SERVICE_NAME: &str = "org.freedesktop.Helmsman";

/// D-Bus 对象挂载路径
pub const DBUS_OBJECT_PATH: &str = "/org/freedesktop/Helmsman";

/// D-Bus 领域契约接口版本 1
pub const DBUS_INTERFACE_V1: &str = "org.freedesktop.Helmsman.v1";

use serde::{Deserialize, Serialize};
use zvariant::Type;

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

impl std::fmt::Display for SystemStatusDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}, {}) - 配置文件: {}",
            self.distro_name, self.distro_family, self.firmware_type, self.config_path
        )
    }
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

/// 已安装主题摘要 DTO
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, zvariant::Type)]
pub struct ThemeInfoDto {
    /// 主题名称
    pub name: String,
    /// 主题目录路径
    pub path: String,
    /// 是否包含 theme.txt
    pub has_descriptor: bool,
}

/// 条目友好显示别名 DTO
///
/// # 语义契约（重要）
/// - **别名只影响 Helmsman 及其客户端的界面展示**，用于给冗长的内核标题起短名。
/// - **绝不会改写** `/boot/grub/grub.cfg` 中的 `menuentry` 真实标题，
///   也不会触碰发行版 `/etc/grub.d/10_linux` 等官方脚本。
/// - 若确实需要修改**开机菜单**上的标题，必须通过受管自定义引导项
///   （`/etc/grub.d/41_helmsman_custom`）覆盖生成对应条目，禁止直接编辑 `grub.cfg`。
///
/// # 设计原理
/// - **实现初衷**：把「界面显示名」与「开机菜单标题」两个易混淆概念在契约层拆开，
///   避免调用方误以为 set_alias 能改 GRUB 菜单。
/// - **核心优势**：内核升级后别名按稳定 ID/原题绑定，不依赖会漂移的菜单序号。
/// - **代价与局限**：别名不会出现在开机菜单上；跨机器导出配置时别名需单独迁移。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct EntryAliasDto {
    /// 稳定条目标识（优先 menuentry id，其次原始标题）
    pub entry_id: String,
    /// 界面显示名（空字符串表示清除别名、回退为原始标题）
    pub display_name: String,
}
