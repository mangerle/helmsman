use crate::dbus_api::{DBUS_OBJECT_PATH, DBUS_SERVICE_NAME, HelmsmanDbusAdapter};
use crate::idle::IdleWatcher;
use crate::service::GrubService;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};
use zbus::connection::Builder;

/// 启动特权 D-Bus 守护进程主事件循环
///
/// # 设计原理
/// - **实现初衷**：配合 systemd 与 D-Bus 系统的按需激活机制（Socket/D-Bus Activation），
///   当桌面客户端发起 D-Bus 调用时系统自动拉起守护进程；在空闲达到阈值后自动安全退出，杜绝无谓的内存与资源常驻。
/// - **核心优势**：基于 tokio 异步事件循环与 zbus 5 的高效调度，支持处理并发请求并平滑监听系统退出信号。
/// - **停机保障**：利用 IdleWatcher 在执行关键配置写入事务时自动加锁阻止退出，避免在事务处理中途发生非正常中断。
///
/// # 参数说明
/// - `service`: GRUB 底层特权服务实例
/// - `idle_timeout_secs`: 连续无请求时自动退出的空闲超时秒数（默认为 60 秒）
pub async fn run_dbus_server(
    service: Arc<GrubService>,
    idle_timeout_secs: u64,
) -> Result<(), zbus::Error> {
    let idle_watcher = Arc::new(IdleWatcher::new());
    let adapter = HelmsmanDbusAdapter::with_idle_watcher(service, Arc::clone(&idle_watcher));

    info!("正在连接 D-Bus 系统总线 (System Bus)...");
    let connection = Builder::system()?
        .name(DBUS_SERVICE_NAME)?
        .serve_at(DBUS_OBJECT_PATH, adapter)?
        .build()
        .await?;

    info!(
        "Helmsman 特权守护服务已成功就绪，服务名: '{}'，对象路径: '{}'",
        DBUS_SERVICE_NAME, DBUS_OBJECT_PATH
    );
    info!("空闲自托管监控已启动，超时阈值: {} 秒", idle_timeout_secs);

    let mut check_interval = tokio::time::interval(Duration::from_secs(5));
    // 首次 tick 立即触发，先跳过
    check_interval.tick().await;

    loop {
        tokio::select! {
            _ = check_interval.tick() => {
                if idle_watcher.is_idle_timeout(idle_timeout_secs) {
                    info!(
                        "检测到连续空闲超过 {} 秒且无在途事务，守护进程开始平滑停机...",
                        idle_timeout_secs
                    );
                    break;
                }
            }
            signal_res = tokio::signal::ctrl_c() => {
                match signal_res {
                    Ok(()) => {
                        info!("接收到系统中断信号 (Ctrl+C / SIGINT)，开始平滑停机...");
                    }
                    Err(e) => {
                        warn!("监听系统停机信号异常: {}，强制退出", e);
                    }
                }
                break;
            }
        }
    }

    info!("注销 D-Bus 对象并安全释放资源...");
    drop(connection);
    info!("Helmsman 特权守护进程已退出。");
    Ok(())
}
