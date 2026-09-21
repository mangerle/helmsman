/// D-Bus 服务端适配器模块
mod adapter;
mod constants;
mod error;

pub use adapter::{
    HelmsmanDbusAdapter, HelmsmanDbusAdapterProxy, HelmsmanDbusAdapterProxyBlocking,
};
pub use constants::{DBUS_INTERFACE_V1, DBUS_OBJECT_PATH, DBUS_SERVICE_NAME, polkit_actions};
pub use error::HelmsmanDbusError;
pub use helmsman_client::{ApplyResultDto, DiffResultDto, SnapshotDto, SystemStatusDto};

#[cfg(test)]
mod tests;
