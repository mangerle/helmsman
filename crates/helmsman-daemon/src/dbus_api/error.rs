use zbus::DBusError;

/// Helmsman D-Bus 服务领域错误枚举
///
/// # 设计原理
/// - **实现初衷**：在 D-Bus 协议层向客户端返回具备明确领域命名空间与中文描述的强类型错误。
/// - **核心优势**：通过 zbus DBusError 宏自动映射为 D-Bus 标准错误报文，支持错误链与模式匹配。
#[derive(DBusError, Debug)]
#[zbus(prefix = "org.freedesktop.Helmsman.Error")]
pub enum HelmsmanDbusError {
    /// 底层 ZBus 通信错误
    #[zbus(error)]
    ZBus(zbus::Error),
    /// 未通过 PolicyKit 权限核验
    NotAuthorized(String),
    /// 传入参数非法
    InvalidArgs(String),
    /// 业务执行失败
    Failed(String),
}
