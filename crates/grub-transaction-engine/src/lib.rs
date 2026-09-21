pub mod atomic;
pub mod diff;
pub mod lock;
pub mod snapshot;

pub use atomic::atomic_write;
pub use diff::{DiffReport, generate_unified_diff};
pub use lock::{
    LockCheckStrategy, LockDescriptor, LockError, PackageManagerType, check_package_manager_locks,
    check_single_lock, default_system_locks,
};
pub use snapshot::{SnapshotMeta, create_snapshot, list_snapshots, restore_snapshot};
