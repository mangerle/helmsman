mod app;
mod backend;
mod theme_actions;
mod update_actions;
mod views;

use app::HelmsmanWindow;
use backend::BackendSnapshot;
use gpui_kit::component::TitleBar;
use gpui_kit::*;
use helmsman_updater::{RecoveryStatus, check_and_recover_on_startup, confirm_update_success};
use tracing::{error, info, warn};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    match check_and_recover_on_startup() {
        Ok(RecoveryStatus::Normal) => {}
        Ok(RecoveryStatus::PendingConfirmation { attempts }) => {
            info!("新版本处于启动观察期，当前尝试次数: {}", attempts);
        }
        Ok(RecoveryStatus::RolledBack { from_version }) => {
            warn!(
                "检测到新版本 {} 启动异常，已自动回滚至上一稳定版本",
                from_version
            );
        }
        Ok(_) => {}
        Err(err) => {
            warn!("启动健康自检失败（继续正常启动）: {err}");
        }
    }

    info!("正在启动 Helmsman 桌面图形界面...");
    let snapshot = BackendSnapshot::load();

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);

            let window_options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1240.0), px(800.0)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Helmsman · Linux GRUB 引导管理器".into()),
                    ..TitleBar::title_bar_options()
                }),
                ..TitleBar::window_options()
            };

            match gpui_kit::open_window(window_options, cx, move |window, cx| {
                cx.new(|cx| {
                    let view = HelmsmanWindow::new(snapshot, window, cx);
                    view.sync_kit_theme(window, cx);
                    view
                })
            }) {
                Ok(_) => {
                    if let Err(err) = confirm_update_success() {
                        warn!("确认升级闭环状态失败: {err}");
                    }
                }
                Err(err) => {
                    error!("创建 Helmsman 桌面主窗口失败: {err}");
                }
            }
        });
}
