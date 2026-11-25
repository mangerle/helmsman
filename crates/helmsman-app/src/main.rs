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
    ensure_desktop_integration();
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
                app_id: Some("helmsman".to_string()),
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

/// 在用户目录注册桌面图标与 `.desktop` 入口，确保 Wayland/X11 Dock 栏正确显示应用 Logo
fn ensure_desktop_integration() {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    let data_home = std::env::var("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(home).join(".local/share"));

    let icon_dir = data_home.join("icons/hicolor/scalable/apps");
    let icon_path = icon_dir.join("helmsman.svg");
    let logo_bytes = include_bytes!("../../../assets/logo.svg");
    if std::fs::read(&icon_path).ok().as_deref() != Some(logo_bytes.as_slice())
        && std::fs::create_dir_all(&icon_dir).is_ok()
    {
        let _ = std::fs::write(&icon_path, logo_bytes);
    }

    let system_desktop = std::path::Path::new("/usr/share/applications/helmsman.desktop");
    let user_desktop_dir = data_home.join("applications");
    let user_desktop = user_desktop_dir.join("helmsman.desktop");
    if !system_desktop.exists() && !user_desktop.exists() {
        let exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.to_str().map(str::to_string))
            .unwrap_or_else(|| "helmsman-app".to_string());
        let desktop_entry = format!(
            "[Desktop Entry]\nType=Application\nName=Helmsman\nGenericName=GRUB 引导配置管理器\nExec={exe}\nIcon={}\nStartupWMClass=helmsman\nTerminal=false\nCategories=System;Settings;\n",
            icon_path.display()
        );
        if std::fs::create_dir_all(&user_desktop_dir).is_ok() {
            let _ = std::fs::write(&user_desktop, desktop_entry);
        }
    }
}
