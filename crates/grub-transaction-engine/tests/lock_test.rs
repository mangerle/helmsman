use grub_transaction_engine::{
    LockDescriptor, LockError, PackageManagerType, check_package_manager_locks, check_single_lock,
};
use std::fs;
use std::path::PathBuf;

/// 测试辅助结构：临时测试沙箱目录
struct TestSandbox {
    dir: PathBuf,
}

impl TestSandbox {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "helmsman_test_lock_{}_{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn test_lock_not_exist_passes() {
    let sandbox = TestSandbox::new("not_exist");
    let non_existent = sandbox.dir.join("non_existent.lck");

    let desc = LockDescriptor::existence(PackageManagerType::Pacman, &non_existent);
    assert!(check_single_lock(&desc).is_ok());
}

#[test]
fn test_pacman_existence_lock() {
    let sandbox = TestSandbox::new("pacman");
    let lock_file = sandbox.dir.join("db.lck");

    let desc = LockDescriptor::existence(PackageManagerType::Pacman, &lock_file);

    // 初始状态：未创建锁文件，检查通过
    assert!(check_single_lock(&desc).is_ok());

    // 创建锁文件，模拟 pacman 正在运行
    fs::write(&lock_file, "12345").unwrap();
    let res = check_single_lock(&desc);
    assert!(res.is_err());
    if let Err(LockError::PackageManagerBusy { manager, .. }) = res {
        assert_eq!(manager, PackageManagerType::Pacman);
    } else {
        panic!("预期返回 PackageManagerBusy 错误");
    }

    // 删除锁文件后应恢复通行
    fs::remove_file(&lock_file).unwrap();
    assert!(check_single_lock(&desc).is_ok());
}

#[test]
fn test_multiple_locks_check() {
    let sandbox = TestSandbox::new("multiple");
    let lock1 = sandbox.dir.join("lock1");
    let lock2 = sandbox.dir.join("lock2");

    let descriptors = vec![
        LockDescriptor::existence(PackageManagerType::Dpkg, &lock1),
        LockDescriptor::existence(PackageManagerType::Rpm, &lock2),
    ];

    // 全空状态
    assert!(check_package_manager_locks(&descriptors).is_ok());

    // 仅第二个被锁
    fs::write(&lock2, "busy").unwrap();
    let res = check_package_manager_locks(&descriptors);
    assert!(res.is_err());

    if let Err(LockError::PackageManagerBusy { manager, .. }) = res {
        assert_eq!(manager, PackageManagerType::Rpm);
    } else {
        panic!("预期报告 Rpm 繁忙");
    }
}

#[test]
fn test_lock_display_and_error() {
    let err = LockError::PackageManagerBusy {
        manager: PackageManagerType::Dpkg,
        lock_path: PathBuf::from("/var/lib/dpkg/lock"),
        reason: "已被占用".to_string(),
    };

    let text = format!("{}", err);
    assert!(text.contains("dpkg/apt"));
    assert!(text.contains("/var/lib/dpkg/lock"));
    assert!(text.contains("已被占用"));
}
