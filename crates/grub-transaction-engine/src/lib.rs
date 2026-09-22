pub mod atomic;
pub mod diff;
pub mod disk;
pub mod lock;
pub mod snapshot;
pub mod theme_archive;
pub mod theme_extractor;

pub use atomic::atomic_write;
pub use diff::{DiffReport, generate_unified_diff};
pub use disk::{DiskSpaceError, check_disk_space};
pub use lock::{
    LockCheckStrategy, LockDescriptor, LockError, PackageManagerType, check_package_manager_locks,
    check_single_lock, default_system_locks,
};
pub use snapshot::{
    SnapshotMeta, create_snapshot, delete_snapshot, export_snapshot, list_snapshots,
    prune_snapshots, restore_snapshot,
};
pub use theme_archive::{
    ArchiveFormat, MAX_ENTRIES_COUNT, MAX_SINGLE_FILE_SIZE, MAX_TOTAL_UNCOMPRESSED_SIZE,
    detect_archive_format, extract_archive_to_dir, find_theme_root_in_dir,
};
pub use theme_extractor::{
    InstalledThemeInfo, ThemeSecurityError, extract_safe_entries, install_theme_directory,
    install_theme_from_archive, list_installed_themes, remove_theme, validate_entry_path,
    validate_theme_directory, validate_theme_name,
};
