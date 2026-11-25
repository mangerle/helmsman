use crate::app::{AppUpdateStatus, HelmsmanWindow};
use gpui_kit::*;
use helmsman_updater::{UpdateInfo, check_update, restart_current_process};

impl HelmsmanWindow {
    /// 后台检查远端 GitHub Releases 最新版本（基于 shipup + Ed25519 验签）
    pub fn check_app_update(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.update_status,
            AppUpdateStatus::Checking | AppUpdateStatus::Installing(_)
        ) {
            return;
        }
        self.update_status = AppUpdateStatus::Checking;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let res = cx
                .background_executor()
                .spawn(async move { check_update(None) })
                .await;

            let _ = this.update(cx, |win, cx| {
                win.update_status = match res {
                    Ok(Some(info)) => AppUpdateStatus::Available(Box::new(info)),
                    Ok(None) => AppUpdateStatus::UpToDate,
                    Err(e) => AppUpdateStatus::Error(e.to_string()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// 下载、验签并安装新版应用包
    pub fn install_app_update(&mut self, info: UpdateInfo, cx: &mut Context<Self>) {
        let target_ver = info.version();
        self.update_status = AppUpdateStatus::Installing(format!("正在下载 v{target_ver}..."));
        cx.notify();

        cx.spawn(async move |this, cx| {
            let ver_for_done = target_ver.clone();
            let res = cx
                .background_executor()
                .spawn(async move {
                    let downloaded = info.download_blocking(|_| {})?;
                    downloaded.install(|_| {})?;
                    Ok::<_, helmsman_updater::UpdaterError>(())
                })
                .await;

            let _ = this.update(cx, |win, cx| {
                win.update_status = match res {
                    Ok(()) => AppUpdateStatus::ReadyToRestart(ver_for_done),
                    Err(e) => AppUpdateStatus::Error(e.to_string()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// 重启当前桌面应用以切换到新版本
    pub fn restart_for_update(&mut self, cx: &mut Context<Self>) {
        let Err(e) = restart_current_process();
        self.update_status = AppUpdateStatus::Error(e.to_string());
        cx.notify();
    }
}
