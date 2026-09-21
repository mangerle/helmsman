use std::collections::HashMap;
use std::env;
use std::sync::atomic::{AtomicI8, Ordering};
use tracing::{debug, info, warn};
use zbus::proxy;
use zvariant::Value;

/// PolicyKit 鉴权标志位
pub const CHECK_AUTH_ALLOW_USER_INTERACTION: u32 = 0x0000_0001;

/// 测试模拟权限原子标志：-1 表示未设置（走真实核验），0 表示拒绝，1 表示允许
static MOCK_POLKIT_OVERRIDE: AtomicI8 = AtomicI8::new(-1);

/// 设置测试环境下的 PolicyKit 模拟权限（完全安全，杜绝 unsafe block）
pub fn set_mock_polkit_allow(allow: Option<bool>) {
    let val = match allow {
        None => -1,
        Some(true) => 1,
        Some(false) => 0,
    };
    MOCK_POLKIT_OVERRIDE.store(val, Ordering::Relaxed);
}

/// PolicyKit 系统 Authority 代理契约
///
/// # 设计原理
/// - **实现初衷**：特权守护进程必须遵循最小权限原则，通过 D-Bus 异步调用系统 PolicyKit 守护服务，
///   验证非特权调用者是否具备执行相应高危操作的权限。
/// - **核心优势**：直接利用 zbus proxy 宏生成类型安全的客户端调用，无需额外引入笨重的第三方外部库。
/// - **局限与权衡**：依赖宿主机运行有 PolicyKit 守护进程 (`polkitd`)；在测试环境中需提供安全的 Mock 旁路。
#[proxy(
    interface = "org.freedesktop.PolicyKit1.Authority",
    default_service = "org.freedesktop.PolicyKit1",
    default_path = "/org/freedesktop/PolicyKit1/Authority"
)]
pub trait PolicyKitAuthority {
    /// 向系统 Authority 发起权限核验请求
    fn check_authorization(
        &self,
        subject: (&str, &HashMap<&str, Value<'_>>),
        action_id: &str,
        details: &HashMap<&str, &str>,
        flags: u32,
        cancellation_id: &str,
    ) -> zbus::Result<(bool, bool, HashMap<String, String>)>;
}

/// 检查调用者是否具备指定的 Polkit 动作权限
///
/// # 参数说明
/// - `connection`: 当前活动的 D-Bus 连接实例
/// - `caller_sender`: 调用者的 D-Bus 唯一名称（例如 `:1.42`）
/// - `action_id`: Polkit 策略中定义的动作标识符（例如 `org.freedesktop.Helmsman.apply-changes`）
/// - `allow_interaction`: 是否允许 Polkit 弹出用户交互密码认证窗口
///
/// # 返回值
/// - `Ok(true)`: 鉴权通过，允许执行
/// - `Ok(false)`: 鉴权被拒绝或用户取消
/// - `Err(...)`: D-Bus 通信异常或 Authority 服务不可用
pub async fn check_polkit_authorization(
    connection: &zbus::Connection,
    caller_sender: &str,
    action_id: &str,
    allow_interaction: bool,
) -> Result<bool, zbus::Error> {
    // 1. 优先检查原子内存模拟开关（用于单测与集成测试）
    let mock_override = MOCK_POLKIT_OVERRIDE.load(Ordering::Relaxed);
    if mock_override >= 0 {
        let allowed = mock_override == 1;
        debug!(
            "检测到内存 Mock Polkit 覆盖值: {}，跳过系统 Polkit 校验",
            allowed
        );
        return Ok(allowed);
    }

    // 2. 检查环境变量模拟开关（用于 CI 环境）
    if let Ok(val) = env::var("HELMSMAN_MOCK_POLKIT_ALLOW") {
        debug!(
            "检测到环境变量 HELMSMAN_MOCK_POLKIT_ALLOW={}，跳过真实 Polkit 校验",
            val
        );
        return Ok(val == "1" || val.eq_ignore_ascii_case("true"));
    }

    debug!(
        "开始进行 Polkit 鉴权核验，调用者: {}，动作: {}",
        caller_sender, action_id
    );

    let authority = PolicyKitAuthorityProxy::new(connection).await?;

    let mut subject_details = HashMap::new();
    subject_details.insert("name", Value::from(caller_sender));
    let subject = ("system-bus-name", &subject_details);

    let details = HashMap::new();
    let flags = if allow_interaction {
        CHECK_AUTH_ALLOW_USER_INTERACTION
    } else {
        0
    };

    match authority
        .check_authorization(subject, action_id, &details, flags, "")
        .await
    {
        Ok((is_authorized, is_challenge, _)) => {
            if is_authorized {
                info!(
                    "Polkit 鉴权通过，调用者: {}，动作: {}",
                    caller_sender, action_id
                );
                Ok(true)
            } else if is_challenge {
                warn!(
                    "Polkit 需要交互认证但未完成，调用者: {}，动作: {}",
                    caller_sender, action_id
                );
                Ok(false)
            } else {
                warn!(
                    "Polkit 鉴权被拒绝，调用者: {}，动作: {}",
                    caller_sender, action_id
                );
                Ok(false)
            }
        }
        Err(e) => {
            warn!(
                "调用 PolicyKit Authority 失败，调用者: {}，原因: {}",
                caller_sender, e
            );
            Err(e)
        }
    }
}
