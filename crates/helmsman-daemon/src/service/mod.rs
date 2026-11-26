mod apply;
pub(crate) mod apply_support;
mod error;
mod grub_scripts;
mod ops;
mod sys_tools;
mod theme_ops;

pub use error::DaemonError;
pub use sys_tools::{ConvertFontParams, GrubInstallMode, ReinstallGrubParams};

use grub_distro_adapter::DistroProfile;
use grub_transaction_engine::{
    DiffReport, LockDescriptor, default_system_locks, generate_unified_diff,
};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// 默认备份根目录
pub const DEFAULT_BACKUP_DIR: &str = "/var/backups/grub-manager";
/// 默认快照历史保留数量上限
pub const DEFAULT_SNAPSHOT_RETENTION: usize = 20;
/// 事务落盘前要求的最小可用磁盘空间（10 MiB）
pub const MIN_REQUIRED_DISK_SPACE_BYTES: u64 = 10 * 1024 * 1024;
/// 引导生成命令默认超时秒数
pub const DEFAULT_UPDATE_TIMEOUT_SECS: u64 = 60;

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
    /// 菜单布局编排配置文件路径
    pub menu_layout_path: PathBuf,
    /// 进程内写操作互斥锁（串行化磁盘写入、快照与引导重建，防止并发竞态）
    write_lock: Mutex<()>,
    /// 当前管理的目标系统根目录（默认 "/"，可切换至外部挂载点离线修复）
    target_root: Mutex<PathBuf>,
}

impl GrubService {
    /// 获取写操作互斥锁（仅在同步事务内持有，严禁跨 await 持有）
    pub(crate) fn acquire_write_lock(&self) -> MutexGuard<'_, ()> {
        self.write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 创建标准系统服务实例
    pub fn new_system_default() -> Self {
        let distro_profile = DistroProfile::detect_current_system();
        let themes_dir = distro_profile.themes_dir();
        Self {
            default_config_path: PathBuf::from("/etc/default/grub"),
            backup_dir: PathBuf::from(DEFAULT_BACKUP_DIR),
            distro_profile,
            lock_descriptors: default_system_locks(),
            themes_dir,
            menu_layout_path: PathBuf::from("/etc/helmsman/menu_layout.json"),
            write_lock: Mutex::new(()),
            target_root: Mutex::new(PathBuf::from("/")),
        }
    }

    /// 创建自定义路径实例（用于单元测试与集成测试）
    pub fn new_with_paths(
        default_config_path: PathBuf,
        backup_dir: PathBuf,
        distro_profile: DistroProfile,
    ) -> Self {
        let themes_dir = backup_dir.join("themes");
        let menu_layout_path = backup_dir.join("menu_layout.json");
        Self {
            default_config_path,
            backup_dir,
            distro_profile,
            lock_descriptors: Vec::new(),
            themes_dir,
            menu_layout_path,
            write_lock: Mutex::new(()),
            target_root: Mutex::new(PathBuf::from("/")),
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
        let cfg_path = self.effective_default_config_path();
        let original =
            fs::read_to_string(&cfg_path).map_err(|e| DaemonError::ConfigReadFailed {
                path: cfg_path,
                reason: e.to_string(),
            })?;
        Ok(generate_unified_diff(&original, new_config))
    }

    /// 获取当前生效的目标系统根目录
    pub fn effective_root(&self) -> PathBuf {
        self.target_root
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// 是否处于非宿主机根路径的外部挂载修复模式
    pub fn is_external_root(&self) -> bool {
        self.effective_root() != std::path::Path::new("/")
    }

    /// 获取当前生效的 `/etc/default/grub` 路径
    pub fn effective_default_config_path(&self) -> PathBuf {
        let root = self.effective_root();
        if root == std::path::Path::new("/") {
            self.default_config_path.clone()
        } else {
            root.join("etc/default/grub")
        }
    }

    /// 获取当前生效的 `/etc/grub.d` 目录路径
    pub fn effective_grub_d_dir(&self) -> PathBuf {
        let root = self.effective_root();
        if root == std::path::Path::new("/") {
            PathBuf::from("/etc/grub.d")
        } else {
            root.join("etc/grub.d")
        }
    }

    /// 获取当前生效的 `grub.cfg` 路径
    pub fn effective_boot_cfg_path(&self) -> PathBuf {
        let root = self.effective_root();
        let rel = self.distro_profile.config_path.trim_start_matches('/');
        if root == std::path::Path::new("/") {
            PathBuf::from(&self.distro_profile.config_path)
        } else {
            root.join(rel)
        }
    }
}
