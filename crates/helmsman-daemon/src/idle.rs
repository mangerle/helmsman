use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 空闲状态观察器
///
/// # 设计原理
/// - **实现初衷**：在 Linux 系统中，特权后台服务长期常驻内存会浪费宝贵的系统资源。
///   结合 systemd 的 D-Bus 按需激活（D-Bus Activation）机制，守护进程在无活跃操作一段时间后应自动平滑退出。
/// - **核心优势**：
///   1. 采用 `AtomicU64` 记录 Unix 秒级时间戳，采用 `AtomicBool` 记录事务忙碌状态，实现完全无锁的零竞争读写。
///   2. 引入 `BusyGuard` RAII 守卫机制，在执行引导生成、快照还原等长耗时关键事务期间强制阻止空闲超时，杜绝事务中途被杀。
/// - **代价与局限**：时间戳精度为秒级，不适用于亚毫秒级的微观超时控制；依赖系统时钟单调性或回退容错。
pub struct IdleWatcher {
    /// 最后活跃时间戳（Unix 秒）
    last_active_epoch_secs: AtomicU64,
    /// 是否正处于不可中断的忙碌事务中
    is_busy: AtomicBool,
}

impl IdleWatcher {
    /// 创建新的空闲观察器并初始化最后活跃时间为当前系统时间
    pub fn new() -> Self {
        Self {
            last_active_epoch_secs: AtomicU64::new(Self::current_epoch_secs()),
            is_busy: AtomicBool::new(false),
        }
    }

    /// 获取当前系统的 Unix 秒数
    fn current_epoch_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs()
    }

    /// 刷新最后活跃时间戳为当前时间
    pub fn touch(&self) {
        self.last_active_epoch_secs
            .store(Self::current_epoch_secs(), Ordering::Release);
    }

    /// 进入忙碌事务状态，返回一个 RAII 守护对象
    ///
    /// 在守卫被丢弃（Drop）之前，`is_idle_timeout` 将始终返回 `false`。
    pub fn enter_busy(&self) -> BusyGuard<'_> {
        self.is_busy.store(true, Ordering::Release);
        BusyGuard(self)
    }

    /// 手动标记退出忙碌状态并刷新活跃时间
    pub(crate) fn exit_busy(&self) {
        self.is_busy.store(false, Ordering::Release);
        self.touch();
    }

    /// 检查是否已达到空闲超时阈值
    ///
    /// # 参数
    /// - `timeout_secs`: 超时秒数阈值。若传入 `0` 则视为禁用空闲退出，恒返回 `false`。
    ///
    /// # 返回值
    /// - `true`: 当前无忙碌任务且距离最后一次活跃已超过 `timeout_secs`。
    /// - `false`: 正在执行事务、超时阈值为 0 或尚未超时。
    pub fn is_idle_timeout(&self, timeout_secs: u64) -> bool {
        if timeout_secs == 0 {
            return false;
        }

        if self.is_busy.load(Ordering::Acquire) {
            return false;
        }

        let last = self.last_active_epoch_secs.load(Ordering::Acquire);
        let now = Self::current_epoch_secs();

        // 考虑系统时钟微调回拨的情况：若当前时间小于记录时间，认为刚刚活跃
        if now < last {
            return false;
        }

        (now - last) >= timeout_secs
    }

    /// 获取距离最后一次活跃已经过去的时长
    pub fn time_since_last_active(&self) -> Duration {
        let last = self.last_active_epoch_secs.load(Ordering::Acquire);
        let now = Self::current_epoch_secs();
        if now >= last {
            Duration::from_secs(now - last)
        } else {
            Duration::ZERO
        }
    }

    /// 显式设置最后活跃时间（仅用于单元测试模拟时光流逝）
    #[cfg(test)]
    pub(crate) fn set_last_active_for_test(&self, epoch_secs: u64) {
        self.last_active_epoch_secs
            .store(epoch_secs, Ordering::Release);
    }
}

impl Default for IdleWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// 忙碌事务 RAII 守卫
pub struct BusyGuard<'a>(&'a IdleWatcher);

impl<'a> Drop for BusyGuard<'a> {
    fn drop(&mut self) {
        self.0.exit_busy();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idle_watcher_initial_state() {
        let watcher = IdleWatcher::new();
        assert!(!watcher.is_idle_timeout(10));
        assert!(!watcher.is_idle_timeout(0));
        assert!(watcher.time_since_last_active() <= Duration::from_secs(1));
    }

    #[test]
    fn test_idle_watcher_timeout_detection() {
        let watcher = IdleWatcher::new();
        let now = IdleWatcher::current_epoch_secs();

        // 模拟 100 秒前有最后一次活动
        watcher.set_last_active_for_test(now - 100);

        // 检查 60 秒超时：应该判定为已超时
        assert!(watcher.is_idle_timeout(60));
        // 检查 120 秒超时：尚未超时
        assert!(!watcher.is_idle_timeout(120));
        // 超时设置为 0：禁用，恒不超时
        assert!(!watcher.is_idle_timeout(0));

        // 刷新活跃后应立即解除超时状态
        watcher.touch();
        assert!(!watcher.is_idle_timeout(60));
    }

    #[test]
    fn test_idle_watcher_busy_guard_blocks_timeout() {
        let watcher = IdleWatcher::new();
        let now = IdleWatcher::current_epoch_secs();

        // 模拟已超过 300 秒无活动
        watcher.set_last_active_for_test(now - 300);
        assert!(watcher.is_idle_timeout(60));

        // 进入忙碌状态
        {
            let _guard = watcher.enter_busy();
            // 即使时间上已经超时，处于忙碌状态也严禁退出
            assert!(!watcher.is_idle_timeout(60));
        }

        // 离开作用域后，BusyGuard drop 会自动触发 exit_busy 并 touch，因此也不再超时
        assert!(!watcher.is_idle_timeout(60));
    }
}
