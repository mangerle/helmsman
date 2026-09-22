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
