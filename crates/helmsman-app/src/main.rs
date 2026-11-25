mod app;
mod backend;
mod views;

use app::HelmsmanWindow;
use backend::BackendSnapshot;
use gpui_kit::*;
use tracing::{error, info};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("正在启动 Helmsman 桌面图形界面...");
    let snapshot = BackendSnapshot::load();

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);

        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1180.0), px(760.0)),
                cx,
            ))),
            ..Default::default()
        };

        if let Err(err) = gpui_kit::open_window(window_options, cx, move |window, cx| {
            let view = HelmsmanWindow::new(snapshot);
            view.sync_kit_theme(window, cx);
            cx.new(|_| view)
        }) {
            error!("创建 Helmsman 桌面主窗口失败: {err}");
        }
    });
}
