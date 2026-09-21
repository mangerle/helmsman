use crate::service::{GrubService, TransactionOptions};
use std::fmt;

/// D-Bus 服务名称
pub const DBUS_SERVICE_NAME: &str = "org.freedesktop.Helmsman";

/// D-Bus 对象挂载路径
pub const DBUS_OBJECT_PATH: &str = "/org/freedesktop/Helmsman";

/// D-Bus 领域契约接口版本 1
pub const DBUS_INTERFACE_V1: &str = "org.freedesktop.Helmsman.v1";

/// Polkit 权限动作定义
pub mod polkit_actions {
    /// 读取系统引导状态与配置
    pub const ACTION_READ: &str = "org.freedesktop.Helmsman.read";
    /// 快速切换默认启动项（仅修改 grubenv）
    pub const ACTION_SET_DEFAULT: &str = "org.freedesktop.Helmsman.set-default";
    /// 提交配置变更并重新编译引导脚本
    pub const ACTION_APPLY_CHANGES: &str = "org.freedesktop.Helmsman.apply-changes";
    /// 回滚配置至指定历史快照
    pub const ACTION_ROLLBACK: &str = "org.freedesktop.Helmsman.rollback";
}

/// 系统引导状态 DTO
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyResultDto {
    /// 事务是否成功完成
    pub success: bool,
    /// 生成的快照 ID
    pub snapshot_id: String,
    /// 引导命令执行日志输出
    pub log_output: String,
    /// 失败时的错误信息
    pub error_message: Option<String>,
}

/// D-Bus 接口领域化契约特型 (org.freedesktop.Helmsman.v1)
///
/// # 设计原理
/// - **实现初衷**：杜绝暴露“写文件/执行命令”等底层提权后门接口，仅对外暴露严格受限的语义操作契约。
/// - **核心优势**：将特权面严格闭合在受控业务逻辑内，符合最小权限原则。
/// - **局限与权衡**：所有高级操作必须在守护进程内部实现，调用方无法直接执行任意自定义脚本。
pub trait HelmsmanDbusV1 {
    /// 查询当前系统与引导适配器状态
    fn get_system_status(&self) -> Result<SystemStatusDto, String>;

    /// 列出所有可用的历史配置快照
    fn list_snapshots(&self) -> Result<Vec<SnapshotDto>, String>;

    /// 比对传入新配置与当前配置的差异
    fn preview_changes(&self, new_config: &str) -> Result<DiffResultDto, String>;

    /// 通过 grubenv 快速切换默认启动项
    fn set_default_entry(&self, entry_id_or_title: &str) -> Result<(), String>;

    /// 提交配置修改事务（自动备份快照、原子写入、语法校验并更新引导）
    fn apply_changes(&self, new_config: &str, reason: &str) -> Result<ApplyResultDto, String>;

    /// 回滚至指定历史快照
    fn rollback_snapshot(&self, snapshot_id: &str) -> Result<(), String>;
}

/// D-Bus 领域契约服务适配器
pub struct HelmsmanDbusAdapter<'a> {
    service: &'a GrubService,
}

impl<'a> HelmsmanDbusAdapter<'a> {
    /// 创建适配器实例
    pub fn new(service: &'a GrubService) -> Self {
        Self { service }
    }
}

impl<'a> HelmsmanDbusV1 for HelmsmanDbusAdapter<'a> {
    fn get_system_status(&self) -> Result<SystemStatusDto, String> {
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

    fn list_snapshots(&self) -> Result<Vec<SnapshotDto>, String> {
        let snapshots = self
            .service
            .get_available_snapshots()
            .map_err(|e| e.to_string())?;

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

    fn preview_changes(&self, new_config: &str) -> Result<DiffResultDto, String> {
        let report = self
            .service
            .preview_diff(new_config)
            .map_err(|e| e.to_string())?;

        Ok(DiffResultDto {
            has_changes: report.has_changes,
            added_lines: report.added_lines,
            removed_lines: report.removed_lines,
            diff_text: report.diff_text,
        })
    }

    fn set_default_entry(&self, entry_id_or_title: &str) -> Result<(), String> {
        if entry_id_or_title.trim().is_empty() {
            return Err("启动项标识或标题不能为空".to_string());
        }

        self.service
            .set_default_entry_fast(entry_id_or_title)
            .map_err(|e| e.to_string())
    }

    fn apply_changes(&self, new_config: &str, reason: &str) -> Result<ApplyResultDto, String> {
        if new_config.trim().is_empty() {
            return Err("提交的配置内容不能为空".to_string());
        }

        let options = TransactionOptions::default();
        let result = self
            .service
            .apply_changes(new_config, reason, &options)
            .map_err(|e| e.to_string())?;

        Ok(ApplyResultDto {
            success: result.success,
            snapshot_id: result.snapshot_id,
            log_output: result.log_output,
            error_message: result.error_message,
        })
    }

    fn rollback_snapshot(&self, snapshot_id: &str) -> Result<(), String> {
        if snapshot_id.trim().is_empty() {
            return Err("快照 ID 不能为空".to_string());
        }

        self.service
            .rollback_to_snapshot(snapshot_id)
            .map_err(|e| e.to_string())
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
    }

    #[test]
    fn test_dbus_adapter_status_and_preview() {
        let (config_file, backup_dir) = get_dbus_test_dir("status_preview");
        fs::write(&config_file, "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n").unwrap();

        let distro_profile = DistroProfile {
            family: DistroFamily::DebianUbuntu,
            name: "Ubuntu Linux".to_string(),
            firmware: FirmwareType::Uefi,
            config_path: "/boot/grub/grub.cfg".to_string(),
            update_command: "update-grub".to_string(),
            command_args: Vec::new(),
            check_command: "grub-script-check".to_string(),
            check_command_args: Vec::new(),
            grubenv_path: "/boot/grub/grubenv".to_string(),
            set_default_command: "grub-set-default".to_string(),
        };

        let service = GrubService::new_with_paths(config_file, backup_dir, distro_profile);
        let adapter = HelmsmanDbusAdapter::new(&service);

        // 1. 获取系统状态
        let status = adapter.get_system_status().unwrap();
        assert_eq!(status.distro_name, "Ubuntu Linux");
        assert_eq!(status.firmware_type, "Uefi");

        // 2. 预览差异
        let diff = adapter
            .preview_changes("GRUB_DEFAULT=0\nGRUB_TIMEOUT=15\n")
            .unwrap();
        assert!(diff.has_changes);
        assert_eq!(diff.added_lines, 1);
        assert_eq!(diff.removed_lines, 1);

        // 3. 空输入防御校验
        let empty_apply = adapter.apply_changes("", "空测试");
        assert!(empty_apply.is_err());

        let empty_set = adapter.set_default_entry("   ");
        assert!(empty_set.is_err());

        let empty_rollback = adapter.rollback_snapshot("");
        assert!(empty_rollback.is_err());
    }
}
