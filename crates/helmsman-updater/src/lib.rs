//! Helmsman 桌面端跨版本自更新封装（基于 `shipup`）。
//!
//! # 设计原理
//! - 应用层仅依赖本 crate 暴露的稳定事件与信息模型，不直接耦合 `shipup` 的 API 细节。
//! - 所有网络检查、下载与安装均提供阻塞入口，由 GPUI 后台线程池调度执行。
//! - 远端清单须通过内置 Ed25519 公钥验签，下载包再做 SHA-256 校验，杜绝未签名更新包。

use shipup::{HealthCheckStatus, Update, UpdateEvent as ShipupEvent, Updater};
use std::convert::Infallible;
use std::time::Duration;
use thiserror::Error;

/// 自更新领域错误
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum UpdaterError {
    /// 更新器构建、检查、下载、安装或崩溃回滚失败
    #[error("自更新操作失败，原因: {reason}")]
    Operation {
        /// 中文错误描述
        reason: String,
    },
}

fn updater_error(context: &str, error: impl std::fmt::Display) -> UpdaterError {
    UpdaterError::Operation {
        reason: format!("{context}: {error}"),
    }
}

/// Helmsman 官方发布包 Ed25519 签名验证公钥（Base64 编码）
///
/// 发布前请通过 `shipup-cli keygen` 生成 Helmsman 专属密钥对，将私钥存入
/// GitHub Secret `SHIPUP_PRIVATE_KEY` 并将对应公钥替换至此处。
pub const UPDATE_PUBLIC_KEY: &str = "REPLACE_WITH_HELMSMAN_ED25519_PUBLIC_KEY==";

/// Helmsman 官方 GitHub Releases 最新版本清单端点
pub const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/mangerle/helmsman/releases/latest/download/latest.json";

/// 构建 `shipup::Updater` 实例
///
/// `file://` 仅用于本地清单核验与测试；正式客户端只接受 HTTPS 端点。
fn build_updater(custom_manifest_url: Option<&str>) -> Result<Updater, UpdaterError> {
    let manifest_url = custom_manifest_url
        .filter(|url| !url.trim().is_empty())
        .unwrap_or(DEFAULT_MANIFEST_URL);

    let allow_file = manifest_url.starts_with("file://");

    Updater::builder()
        .current_version(env!("CARGO_PKG_VERSION"))
        .map_err(|error| updater_error("解析当前客户端版本失败", error))?
        .manifest_url(manifest_url)
        .public_key(UPDATE_PUBLIC_KEY)
        .allow_file_protocol(allow_file)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| updater_error("构建自更新引擎失败", error))
}

/// 同步检查是否有可用新版本（供 GPUI 后台线程池调用）
///
/// # Errors
/// 当网络超时、Manifest 解析异常或签名校验失败时返回 [`UpdaterError`]。
pub fn check_update(custom_manifest_url: Option<&str>) -> Result<Option<UpdateInfo>, UpdaterError> {
    let updater = build_updater(custom_manifest_url)?;
    updater
        .check()
        .map(|update| update.map(UpdateInfo::new))
        .map_err(|error| updater_error("检查更新失败", error))
}

/// 已确认可用的新版本信息
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    inner: Update,
}

impl UpdateInfo {
    fn new(inner: Update) -> Self {
        Self { inner }
    }

    /// 新版本号
    pub fn version(&self) -> String {
        self.inner.version().to_string()
    }

    /// 更新日志
    pub fn notes(&self) -> Option<String> {
        self.inner.notes().map(str::to_string)
    }

    /// 发布日期
    pub fn pub_date(&self) -> Option<String> {
        self.inner.pub_date().map(str::to_string)
    }

    /// 同步下载并验签；回调只接收本 crate 的稳定事件类型
    ///
    /// # Errors
    /// 网络失败、哈希或签名校验失败时返回 [`UpdaterError`]。
    pub fn download_blocking<F>(&self, mut callback: F) -> Result<DownloadedUpdate, UpdaterError>
    where
        F: FnMut(UpdateEvent),
    {
        self.inner
            .download(|event| callback(map_event(event)))
            .map(DownloadedUpdate::new)
            .map_err(|error| updater_error("下载或验签更新包失败", error))
    }
}

/// 已下载并验签、等待安装的更新包
#[derive(Debug, Clone)]
pub struct DownloadedUpdate {
    inner: shipup::DownloadedUpdate,
}

impl DownloadedUpdate {
    fn new(inner: shipup::DownloadedUpdate) -> Self {
        Self { inner }
    }

    /// 目标版本号
    pub fn version(&self) -> String {
        self.inner.version().to_string()
    }

    /// 执行物理安装
    ///
    /// # Errors
    /// 归档解压、二进制替换或安装器派生失败时返回 [`UpdaterError`]。
    pub fn install<F>(&self, mut callback: F) -> Result<(), UpdaterError>
    where
        F: FnMut(UpdateEvent),
    {
        self.inner
            .install(|event| callback(map_event(event)))
            .map_err(|error| updater_error("应用更新包失败", error))
    }
}

/// 更新下载/安装阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UpdateStage {
    CheckingManifest,
    DownloadStarted,
    VerifyingChecksum,
    VerifyingSignature,
    ExtractingArchive,
    Installing,
    ReadyToRestart,
    DeferredToReboot,
    Completed,
}

/// 自更新进度事件
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum UpdateEvent {
    DownloadProgress {
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
        percent: Option<f32>,
        speed_bytes_per_sec: Option<u64>,
    },
    Stage(UpdateStage),
    Retrying {
        attempt: u32,
        max_retries: u32,
        error: String,
    },
    Failed {
        reason: String,
    },
}

fn map_event(event: ShipupEvent) -> UpdateEvent {
    match event {
        ShipupEvent::CheckingManifest => UpdateEvent::Stage(UpdateStage::CheckingManifest),
        ShipupEvent::DownloadStarted { .. } => UpdateEvent::Stage(UpdateStage::DownloadStarted),
        ShipupEvent::DownloadProgress {
            downloaded_bytes,
            total_bytes,
            percent,
            speed_bytes_per_sec,
            ..
        } => UpdateEvent::DownloadProgress {
            downloaded_bytes,
            total_bytes,
            percent,
            speed_bytes_per_sec,
        },
        ShipupEvent::VerifyingChecksum => UpdateEvent::Stage(UpdateStage::VerifyingChecksum),
        ShipupEvent::VerifyingSignature => UpdateEvent::Stage(UpdateStage::VerifyingSignature),
        ShipupEvent::ExtractingArchive => UpdateEvent::Stage(UpdateStage::ExtractingArchive),
        ShipupEvent::Installing => UpdateEvent::Stage(UpdateStage::Installing),
        ShipupEvent::ReadyToRestart => UpdateEvent::Stage(UpdateStage::ReadyToRestart),
        ShipupEvent::DeferredToReboot { .. } => UpdateEvent::Stage(UpdateStage::DeferredToReboot),
        ShipupEvent::Retrying {
            attempt,
            max_retries,
            error,
            ..
        } => UpdateEvent::Retrying {
            attempt,
            max_retries,
            error,
        },
        ShipupEvent::Failed { reason } => UpdateEvent::Failed { reason },
        ShipupEvent::Completed => UpdateEvent::Stage(UpdateStage::Completed),
    }
}

/// 启动健康自检结果
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecoveryStatus {
    Normal,
    PendingConfirmation { attempts: u32 },
    RolledBack { from_version: String },
}

/// 客户端启动入口的崩溃自检与自动回滚
///
/// # Errors
/// 回滚覆盖可执行文件失败时返回 [`UpdaterError`]。
pub fn check_and_recover_on_startup() -> Result<RecoveryStatus, UpdaterError> {
    let status = shipup::check_and_recover_current(2)
        .map_err(|error| updater_error("启动自检与崩溃回滚执行失败", error))?;
    Ok(match status {
        HealthCheckStatus::Normal => RecoveryStatus::Normal,
        HealthCheckStatus::PendingConfirmation { attempts } => {
            RecoveryStatus::PendingConfirmation { attempts }
        }
        HealthCheckStatus::RolledBack { from_version } => {
            RecoveryStatus::RolledBack { from_version }
        }
    })
}

/// 客户端主流程平稳就绪后确认升级成功闭环
///
/// # Errors
/// 状态文件清理失败时返回 [`UpdaterError`]。
pub fn confirm_update_success() -> Result<(), UpdaterError> {
    shipup::confirm_update_success()
        .map(|_| ())
        .map_err(|error| updater_error("确认升级状态闭环失败", error))
}

/// 拉起当前可执行文件的新进程并退出本进程
///
/// # Errors
/// 拉起新进程失败时返回 [`UpdaterError`]。
pub fn restart_current_process() -> Result<Infallible, UpdaterError> {
    shipup::restart_with(|_| {}).map_err(|error| updater_error("重启应用失败", error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipup_event_mapping_is_stable() {
        let progress = map_event(ShipupEvent::DownloadProgress {
            downloaded_bytes: 10,
            total_bytes: Some(100),
            percent: Some(10.0),
            speed_bytes_per_sec: Some(1024),
            eta: None,
        });
        assert_eq!(
            progress,
            UpdateEvent::DownloadProgress {
                downloaded_bytes: 10,
                total_bytes: Some(100),
                percent: Some(10.0),
                speed_bytes_per_sec: Some(1024),
            }
        );
    }
}
