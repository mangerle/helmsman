use std::error::Error;
use std::fmt;
use std::fs::OpenOptions;
use std::path::PathBuf;
use tracing::{debug, warn};

/// 包管理器类型枚举
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageManagerType {
    /// Debian / Ubuntu 系列 (dpkg, apt)
    Dpkg,
    /// Red Hat / Fedora 系列 (rpm, dnf)
    Rpm,
    /// Arch Linux 系列 (pacman)
    Pacman,
    /// 自定义包管理器
    Custom(String),
}

impl fmt::Display for PackageManagerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackageManagerType::Dpkg => write!(f, "dpkg/apt (Debian/Ubuntu)"),
            PackageManagerType::Rpm => write!(f, "rpm/dnf (Fedora/RHEL)"),
            PackageManagerType::Pacman => write!(f, "pacman (Arch Linux)"),
            PackageManagerType::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// 锁检测策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockCheckStrategy {
    /// 基于文件存在性判定（如 pacman 的 db.lck，只要文件存在即代表锁定）
    FileExistence,
    /// 基于独占写入判定（如 dpkg/rpm 锁文件，若已有进程独占加锁则打开失败）
    ExclusiveOpen,
}

/// 锁描述符
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockDescriptor {
    /// 对应的包管理器类型
    pub manager: PackageManagerType,
    /// 锁文件的绝对路径
    pub path: PathBuf,
    /// 锁判定策略
    pub strategy: LockCheckStrategy,
}

impl LockDescriptor {
    /// 创建基于文件存在判定的描述符
    pub fn existence(manager: PackageManagerType, path: impl Into<PathBuf>) -> Self {
        Self {
            manager,
            path: path.into(),
            strategy: LockCheckStrategy::FileExistence,
        }
    }

    /// 创建基于独占写入判定的描述符
    pub fn exclusive(manager: PackageManagerType, path: impl Into<PathBuf>) -> Self {
        Self {
            manager,
            path: path.into(),
            strategy: LockCheckStrategy::ExclusiveOpen,
        }
    }
}

/// 包管理器并发互斥锁冲突错误
#[derive(Debug, PartialEq, Eq)]
pub enum LockError {
    /// 检测到包管理器正在运行
    PackageManagerBusy {
        manager: PackageManagerType,
        lock_path: PathBuf,
        reason: String,
    },
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockError::PackageManagerBusy {
                manager,
                lock_path,
                reason,
            } => {
                write!(
                    f,
                    "检测到包管理器 {} 正在运行，锁文件: {}，原因: {}",
                    manager,
                    lock_path.display(),
                    reason
                )
            }
        }
    }
}

impl Error for LockError {}

/// 获取系统默认监控的包管理器锁列表
pub fn default_system_locks() -> Vec<LockDescriptor> {
    vec![
        // Debian / Ubuntu
        LockDescriptor::exclusive(PackageManagerType::Dpkg, "/var/lib/dpkg/lock-frontend"),
        LockDescriptor::exclusive(PackageManagerType::Dpkg, "/var/lib/dpkg/lock"),
        LockDescriptor::exclusive(PackageManagerType::Dpkg, "/var/lib/apt/lists/lock"),
        // Red Hat / Fedora
        LockDescriptor::exclusive(PackageManagerType::Rpm, "/var/lib/rpm/.rpm.lock"),
        // Arch Linux
        LockDescriptor::existence(PackageManagerType::Pacman, "/var/lib/pacman/db.lck"),
    ]
}

/// 检查单个锁描述符的状态
///
/// # 设计原理
/// - **实现初衷**：在修改系统引导配置前，排查是否有包管理器正在更新内核或引导组件，杜绝并发竞争。
/// - **核心优势**：零依赖纯安全 Rust 实现，不阻塞任何外部进程，微秒级完成探测。
/// - **代价与局限**：对于某些仅通过 Unix 套接字通信但未持有文件锁的冷门包管理器可能无法完全涵盖。
///
/// # Errors
/// 当检测到任何锁被占用时，返回 `LockError::PackageManagerBusy`。
pub fn check_single_lock(desc: &LockDescriptor) -> Result<(), LockError> {
    match desc.strategy {
        LockCheckStrategy::FileExistence => {
            if desc.path.exists() {
                warn!(
                    "包管理器锁文件存在，判定为繁忙: {} ({})",
                    desc.path.display(),
                    desc.manager
                );
                return Err(LockError::PackageManagerBusy {
                    manager: desc.manager.clone(),
                    lock_path: desc.path.clone(),
                    reason: "锁文件已存在，表明正在执行包管理事务".to_string(),
                });
            }
        }
        LockCheckStrategy::ExclusiveOpen => {
            if desc.path.exists() {
                // 尝试以写入模式打开，检测是否被独占
                match OpenOptions::new().write(true).open(&desc.path) {
                    Ok(_) => {
                        debug!("锁文件未被独占锁定: {}", desc.path.display());
                    }
                    Err(e) if is_lock_contention(&e) => {
                        warn!(
                            "无法获取锁文件的独占访问权: {} ({})",
                            desc.path.display(),
                            desc.manager
                        );
                        return Err(LockError::PackageManagerBusy {
                            manager: desc.manager.clone(),
                            lock_path: desc.path.clone(),
                            reason: format!("锁文件已被其他进程占用: {}", e),
                        });
                    }
                    Err(e) => {
                        // 权限不足或其他错误（在非特权检测或文件不存在时安全跳过）
                        debug!(
                            "锁文件打开异常（可能权限不足）: {}，原因: {}",
                            desc.path.display(),
                            e
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

/// 检查一组包管理器锁
///
/// # Errors
/// 当任意受检锁被占用时，返回 `LockError::PackageManagerBusy`。
pub fn check_package_manager_locks(descriptors: &[LockDescriptor]) -> Result<(), LockError> {
    for desc in descriptors {
        check_single_lock(desc)?;
    }
    Ok(())
}

/// 判定 IO 错误是否代表锁争用
fn is_lock_contention(err: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    match err.kind() {
        ErrorKind::PermissionDenied | ErrorKind::WouldBlock => true,
        _ => {
            #[cfg(unix)]
            {
                if let Some(raw_os_error) = err.raw_os_error() {
                    // EACCES (13), EAGAIN (11), EWOULDBLOCK (11), EBUSY (16)
                    return raw_os_error == 13 || raw_os_error == 11 || raw_os_error == 16;
                }
            }
            #[cfg(windows)]
            {
                if let Some(raw_os_error) = err.raw_os_error() {
                    // ERROR_SHARING_VIOLATION (32), ERROR_LOCK_VIOLATION (33)
                    return raw_os_error == 32 || raw_os_error == 33;
                }
            }
            false
        }
    }
}
