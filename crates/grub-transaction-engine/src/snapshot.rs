use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;

/// 快照元数据
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotMeta {
    /// 唯一快照 ID
    pub id: String,
    /// 创建时间戳 (UNIX 秒)
    pub timestamp: u64,
    /// 触发原因说明（例如："修改配置前自动备份"）
    pub reason: String,
    /// 目标文件绝对路径
    pub target_file: PathBuf,
    /// 备份副本所在路径
    pub backup_file: PathBuf,
}

impl SnapshotMeta {
    /// 序列化为简单键值对描述文件
    pub fn save_info(&self, dir: &Path) -> io::Result<()> {
        let content = format!(
            "id={}\ntimestamp={}\nreason={}\ntarget={}\nbackup={}\n",
            self.id,
            self.timestamp,
            self.reason,
            self.target_file.display(),
            self.backup_file.display()
        );
        fs::write(dir.join("info.txt"), content)
    }

    /// 从 info.txt 反序列化
    pub fn load_info(dir: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(dir.join("info.txt"))?;
        let mut id = String::new();
        let mut timestamp = 0u64;
        let mut reason = String::new();
        let mut target_file = PathBuf::new();
        let mut backup_file = PathBuf::new();

        for line in content.lines() {
            if let Some(val) = line.strip_prefix("id=") {
                id = val.to_string();
            } else if let Some(val) = line.strip_prefix("timestamp=") {
                timestamp = val.parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("reason=") {
                reason = val.to_string();
            } else if let Some(val) = line.strip_prefix("target=") {
                target_file = PathBuf::from(val);
            } else if let Some(val) = line.strip_prefix("backup=") {
                backup_file = PathBuf::from(val);
            }
        }

        Ok(Self {
            id,
            timestamp,
            reason,
            target_file,
            backup_file,
        })
    }
}

/// 为指定配置文件创建快照副本并写入元数据
///
/// # 设计原理
/// - **实现初衷**：在修改系统级引导文件前提供强一致性的灾难备份，确保任何意外均可逆。
/// - **核心优势**：以时间戳与进程 PID 组合为快照目录唯一 ID，并在目录内留存 `info.txt` 元数据与原始副本，
///   自包含设计便于外部排查或通过文件系统直接手工抢救。
/// - **代价与局限**：占用 `/var/backups` 少量磁盘空间，长期运行需搭配定期快照轮转清理机制。
///
/// # Errors
/// - 当待备份的目标文件不存在时返回 `io::ErrorKind::NotFound`；
/// - 当快照目录创建、文件复制或元数据写入失败时返回 `io::Error`。
pub fn create_snapshot(
    target_file: &Path,
    backup_base_dir: &Path,
    reason: &str,
) -> io::Result<SnapshotMeta> {
    if !target_file.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("目标文件不存在: {}", target_file.display()),
        ));
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let id = format!("snapshot_{}_{}", now, std::process::id());
    let snapshot_dir = backup_base_dir.join(&id);
    fs::create_dir_all(&snapshot_dir)?;

    let file_name = target_file
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("target"));
    let backup_file = snapshot_dir.join(file_name);

    // 复制原始内容至快照副本
    fs::copy(target_file, &backup_file)?;

    let meta = SnapshotMeta {
        id,
        timestamp: now,
        reason: reason.to_string(),
        target_file: target_file.to_path_buf(),
        backup_file,
    };

    meta.save_info(&snapshot_dir)?;
    debug!(
        "快照创建完成，ID: {}，目标: {}",
        meta.id,
        target_file.display()
    );
    Ok(meta)
}

/// 从快照中还原文件至目标路径
///
/// # 设计原理
/// - **实现初衷**：实现一键灾难恢复，当引导生成命令失败或用户主动回滚时将配置复原。
/// - **核心优势**：直接覆盖还原，不产生中间多余分支。
///
/// # Errors
/// - 当备份文件不存在时返回 `io::ErrorKind::NotFound`；
/// - 当文件复制操作被操作系统拒绝或磁盘满时返回 `io::Error`。
pub fn restore_snapshot(meta: &SnapshotMeta) -> io::Result<()> {
    if !meta.backup_file.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("备份文件丢失: {}", meta.backup_file.display()),
        ));
    }
    fs::copy(&meta.backup_file, &meta.target_file)?;
    debug!("快照恢复完成，目标: {}", meta.target_file.display());
    Ok(())
}

/// 列出备份目录下所有可用快照并按时间戳降序排列
///
/// # 设计原理
/// - **实现初衷**：为前端和命令行提供历史快照列表与回滚目标选择。
/// - **核心优势**：自动过滤损坏或未完全初始化的快照目录；最新快照自动置顶。
///
/// # Errors
/// - 当备份根目录读取失败或权限不足时返回 `io::Error`。
pub fn list_snapshots(backup_base_dir: &Path) -> io::Result<Vec<SnapshotMeta>> {
    let mut snapshots = Vec::new();
    if !backup_base_dir.exists() {
        return Ok(snapshots);
    }

    for entry in fs::read_dir(backup_base_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir()
            && path.join("info.txt").exists()
            && let Ok(meta) = SnapshotMeta::load_info(&path)
        {
            snapshots.push(meta);
        }
    }

    // 按时间降序排序（最新快照排在最前）
    snapshots.sort_by_key(|b| std::cmp::Reverse(b.timestamp));
    Ok(snapshots)
}
