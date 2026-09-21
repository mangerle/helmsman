use helmsman_daemon::{GrubService, run_dbus_server};
use std::process;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt};

/// 空闲超时秒数：连续无 D-Bus 请求后守护进程自动平滑退出
const IDLE_TIMEOUT_SECS: u64 = 60;

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).init();

    let service = Arc::new(GrubService::new_system_default());
    info!("正在以 D-Bus 守护进程模式启动...");

    if let Err(e) = run_dbus_server(service, IDLE_TIMEOUT_SECS).await {
        error!("D-Bus 守护进程异常退出: {}", e);
        process::exit(1);
    }
}
