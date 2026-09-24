//! Helmsman 非特权客户端契约层
//!
//! # 设计原理
//! - **实现初衷**：图形界面与第三方工具不应依赖特权守护进程 crate。
//!   本 crate 仅暴露 D-Bus 接口常量、DTO 与类型安全代理，UI 保持无特权编译图。
//! - **核心优势**：daemon 负责服务端与 Polkit，client 负责契约消费，边界清晰。
//! - **代价与局限**：契约变更需同步 policy 与 daemon 接口实现。

pub mod actions;
pub mod proxy;
pub mod types;

pub use actions::polkit_actions;
pub use proxy::{HelmsmanApiProxy, HelmsmanApiProxyBlocking};
pub use types::{
    ApplyResultDto, DBUS_INTERFACE_V1, DBUS_OBJECT_PATH, DBUS_SERVICE_NAME, DiffResultDto,
    EntryAliasDto, SnapshotDto, SystemStatusDto, ThemeInfoDto,
};
