mod apply;
mod apply_support;
mod error;
mod ops;

pub use error::DaemonError;

use grub_distro_adapter::DistroProfile;
use grub_transaction_engine::{
    DiffReport, LockDescriptor, default_system_locks, generate_unified_diff,
};
use std::fs;
use std::path::PathBuf;

/// 事务应用选项配置
#[derive(Debug, Clone, Default)]
pub struct TransactionOptions {
    /// 是否跳过实际引导生成命令的执行（用于单元测试与模拟演练）
    pub skip_command_execution: bool,
    /// 是否跳过引导脚本语法检查（用于测试模拟）
    pub skip_syntax_check: bool,
    /// 引导生成任务设限超时秒数（默认为 60 秒）
    pub timeout_seconds: Option<u64>,
}

/// 事务应用结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionResult {
    /// 事务是否成功提交
    pub success: bool,
    /// 生成的快照 ID
    pub snapshot_id: String,
    /// 引导生成命令的控制台输出日志
    pub log_output: String,
    /// 失败时的错误原因
    pub error_message: Option<String>,
}

/// GRUB 特权业务协调服务
pub struct GrubService {
    /// /etc/default/grub 路径
    pub default_config_path: PathBuf,
    /// 备份根目录
    pub backup_dir: PathBuf,
    /// 跨发行版档案
    pub distro_profile: DistroProfile,
    /// 需检测的包管理器锁描述符列表
    pub lock_descriptors: Vec<LockDescriptor>,
    /// 主题存放根目录
    pub themes_dir: PathBuf,
}

impl GrubService {
    /// 创建标准系统服务实例
    pub fn new_system_default() -> Self {
        let distro_profile = DistroProfile::detect_current_system();
        let themes_dir = distro_profile.themes_dir();
        Self {
            default_config_path: PathBuf::from("/etc/default/grub"),
            backup_dir: PathBuf::from("/var/backups/grub-manager"),
            distro_profile,
            lock_descriptors: default_system_locks(),
            themes_dir,
        }
    }

    /// 创建自定义路径实例（用于单元测试与集成测试）
    pub fn new_with_paths(
        default_config_path: PathBuf,
        backup_dir: PathBuf,
        distro_profile: DistroProfile,
    ) -> Self {
        let themes_dir = backup_dir.join("themes");
        Self {
            default_config_path,
            backup_dir,
            distro_profile,
            lock_descriptors: Vec::new(),
            themes_dir,
        }
    }

    /// 链式配置主题根目录（用于测试隔离）
    pub fn with_themes_dir(mut self, themes_dir: PathBuf) -> Self {
        self.themes_dir = themes_dir;
        self
    }

    /// 链式配置自定义锁描述符列表（用于测试或非标准环境）
    pub fn with_lock_descriptors(mut self, descriptors: Vec<LockDescriptor>) -> Self {
        self.lock_descriptors = descriptors;
        self
    }

    /// 预览配置差异 (Diff)
    ///
    /// # Errors
    /// 当目标配置文件不存在或无法读取时返回 [`DaemonError::ConfigReadFailed`]。
    pub fn preview_diff(&self, new_config: &str) -> Result<DiffReport, DaemonError> {
        let original = fs::read_to_string(&self.default_config_path).map_err(|e| {
            DaemonError::ConfigReadFailed {
                path: self.default_config_path.clone(),
                reason: e.to_string(),
            }
        })?;
        Ok(generate_unified_diff(&original, new_config))
    }
}
