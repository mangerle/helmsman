pub mod audit;
pub mod custom_manager;
pub mod dbus_api;
pub mod executor;
pub mod idle;
pub mod polkit;
pub mod server;
pub mod service;

pub use audit::{
    AuditAction, AuditEvent, record_audit_event, resolve_caller_uid, resolve_caller_uid_from_bus,
};
pub use custom_manager::{CustomManager, DEFAULT_ALIASES_PATH, DEFAULT_CUSTOM_SCRIPT_PATH};
pub use dbus_api::{
    ApplyResultDto, DBUS_INTERFACE_V1, DBUS_OBJECT_PATH, DBUS_SERVICE_NAME, DiffResultDto,
    HelmsmanDbusAdapter, HelmsmanDbusAdapterProxy, HelmsmanDbusAdapterProxyBlocking,
    HelmsmanDbusError, SnapshotDto, SystemStatusDto, ThemeInfoDto, polkit_actions,
};
pub use executor::{SafeCommand, SecurityError};
pub use idle::{BusyGuard, IdleWatcher};
#[cfg(feature = "test-support")]
pub use polkit::set_mock_polkit_allow;
pub use polkit::{PolicyKitAuthorityProxy, check_polkit_authorization};
pub use server::run_dbus_server;
pub use service::{DaemonError, GrubService, TransactionOptions, TransactionResult};

// 契约层再导出：便于既有测试与调用方平滑迁移
pub use helmsman_client as client;
