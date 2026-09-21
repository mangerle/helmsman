use grub_boot_reader::CustomBootEntry;
use grub_distro_adapter::{DistroFamily, DistroProfile, FirmwareType};
use helmsman_daemon::{
    CustomManager, DBUS_OBJECT_PATH, GrubService, HelmsmanDbusAdapter, HelmsmanDbusAdapterProxy,
    TransactionOptions, set_mock_polkit_allow,
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use zbus::connection;

fn get_p2p_test_env(name: &str) -> (PathBuf, PathBuf, GrubService, CustomManager) {
    let base = std::env::temp_dir()
        .join("helmsman_p2p_dbus_test")
        .join(name);
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();

    let config_file = base.join("default_grub");
    let backup_dir = base.join("backups");
    let custom_script = base.join("41_helmsman_custom");
    let aliases_file = base.join("aliases.json");
    fs::create_dir_all(&backup_dir).unwrap();

    fs::write(&config_file, "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n").unwrap();

    let distro_profile = DistroProfile {
        family: DistroFamily::DebianUbuntu,
        name: "Ubuntu Linux".to_string(),
        firmware: FirmwareType::Uefi,
        config_path: "/boot/grub/grub.cfg".to_string(),
        update_command: "update-grub".to_string(),
        command_args: Vec::new(),
        check_command: "grub-script-check".to_string(),
        check_command_args: Vec::new(),
        grubenv_path: "/boot/grub/grubenv".to_string(),
        set_default_command: "echo".to_string(),
    };

    let service =
        GrubService::new_with_paths(config_file.clone(), backup_dir.clone(), distro_profile);
    let custom_manager = CustomManager::new_with_paths(custom_script, aliases_file, backup_dir);
    (config_file, base, service, custom_manager)
}

async fn setup_p2p_dbus_pair(
    service: Arc<GrubService>,
    custom_manager: Arc<CustomManager>,
) -> (connection::Connection, connection::Connection) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server_task = tokio::spawn(async move {
        let (server_stream, _) = listener.accept().await.unwrap();
        let options = TransactionOptions {
            skip_command_execution: true,
            ..Default::default()
        };
        let adapter = HelmsmanDbusAdapter::new(service)
            .with_custom_manager(custom_manager)
            .with_options(options);
        let guid = zbus::Guid::generate();
        connection::Builder::tcp_stream(server_stream)
            .server(guid)
            .unwrap()
            .p2p()
            .serve_at(DBUS_OBJECT_PATH, adapter)
            .unwrap()
            .build()
            .await
            .unwrap()
    });

    let client_stream = TcpStream::connect(addr).await.unwrap();
    let client_conn = connection::Builder::tcp_stream(client_stream)
        .p2p()
        .build()
        .await
        .unwrap();

    let server_conn = server_task.await.unwrap();
    (server_conn, client_conn)
}

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn test_dbus_p2p_end_to_end_authorized_apply() {
    let _guard = TEST_LOCK.lock().await;
    set_mock_polkit_allow(Some(true));

    let (config_file, _base, service, custom_manager) = get_p2p_test_env("authorized_apply");
    let (_server_conn, client_conn) =
        setup_p2p_dbus_pair(Arc::new(service), Arc::new(custom_manager)).await;

    // 1. 客户端通过自动派生的 HelmsmanDbusAdapterProxy 发起 D-Bus 远程调用
    let proxy = HelmsmanDbusAdapterProxy::builder(&client_conn)
        .path(DBUS_OBJECT_PATH)
        .unwrap()
        .build()
        .await
        .unwrap();

    // 2. 验证只读系统状态获取
    let status = proxy.get_system_status().await.unwrap();
    assert_eq!(status.distro_name, "Ubuntu Linux");
    assert_eq!(status.firmware_type, "Uefi");

    // 3. 验证差异预览
    let new_cfg = "GRUB_DEFAULT=2\nGRUB_TIMEOUT=10\n";
    let diff = proxy.preview_changes(new_cfg).await.unwrap();
    assert!(diff.has_changes);
    assert_eq!(diff.added_lines, 2);
    assert_eq!(diff.removed_lines, 2);

    // 4. 验证鉴权通过时的特权配置写入事务
    let apply_res = proxy
        .apply_changes(new_cfg, "单元测试非特权调用提交")
        .await
        .unwrap();

    assert!(apply_res.success);
    assert!(!apply_res.snapshot_id.is_empty());
    assert!(apply_res.error_message.is_empty());

    // 5. 验证底层磁盘文件确实被特权服务端原子替换生效
    let written = fs::read_to_string(&config_file).unwrap();
    assert_eq!(written, new_cfg);

    // 6. 验证可通过 D-Bus 列出生成的快照
    let snapshots = proxy.list_snapshots().await.unwrap();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id, apply_res.snapshot_id);
    assert_eq!(snapshots[0].reason, "单元测试非特权调用提交");
}

#[tokio::test]
async fn test_dbus_p2p_end_to_end_not_authorized_blocked() {
    let _guard = TEST_LOCK.lock().await;
    set_mock_polkit_allow(Some(false));

    let (config_file, _base, service, custom_manager) = get_p2p_test_env("unauthorized_blocked");
    let original_content = fs::read_to_string(&config_file).unwrap();
    let (_server_conn, client_conn) =
        setup_p2p_dbus_pair(Arc::new(service), Arc::new(custom_manager)).await;

    let proxy = HelmsmanDbusAdapterProxy::builder(&client_conn)
        .path(DBUS_OBJECT_PATH)
        .unwrap()
        .build()
        .await
        .unwrap();

    // 1. 只读操作不受 Polkit 阻断
    let status = proxy.get_system_status().await;
    assert!(status.is_ok());

    // 2. 特权写操作必须被 Polkit 鉴权拦截，返回 NotAuthorized 错误
    let new_cfg = "GRUB_DEFAULT=99\nGRUB_TIMEOUT=0\n";
    let apply_res = proxy.apply_changes(new_cfg, "未授权的恶意调用").await;

    assert!(apply_res.is_err(), "未授权的写操作必须被拒绝");
    let err_str = apply_res.err().unwrap().to_string();
    assert!(
        err_str.contains("未通过管理员身份验证") || err_str.contains("NotAuthorized"),
        "错误信息应明确指示未授权，实际: {}",
        err_str
    );

    // 3. 验证底层磁盘配置文件保持原样，未被破坏
    let current_content = fs::read_to_string(&config_file).unwrap();
    assert_eq!(current_content, original_content);
}

#[tokio::test]
async fn test_dbus_p2p_custom_entries_and_aliases() {
    let _guard = TEST_LOCK.lock().await;
    set_mock_polkit_allow(Some(true));

    let (_config_file, _base, service, custom_manager) = get_p2p_test_env("custom_and_aliases");
    let custom_script_path = custom_manager.custom_script_path.clone();
    let (_server_conn, client_conn) =
        setup_p2p_dbus_pair(Arc::new(service), Arc::new(custom_manager)).await;

    let proxy = HelmsmanDbusAdapterProxy::builder(&client_conn)
        .path(DBUS_OBJECT_PATH)
        .unwrap()
        .build()
        .await
        .unwrap();

    // 1. 获取初始自定义列表（应为空）
    let initial_custom = proxy.get_custom_entries().await.unwrap();
    assert!(initial_custom.is_empty());

    // 2. 通过 D-Bus 提交自定义条目（ISO 镜像与 Windows 链式加载）
    let iso_entry = CustomBootEntry::new_iso_boot(
        "ubuntu_iso",
        "Ubuntu Live ISO",
        "/boot/iso/ubuntu.iso",
        "UUID-1122",
        "nomodeset",
    );
    let win_entry = CustomBootEntry::new_chainloader(
        "win11",
        "Windows 11",
        "/EFI/Microsoft/Boot/bootmgfw.efi",
        "UUID-3344",
    );

    let apply_res = proxy
        .apply_custom_entries(vec![iso_entry, win_entry], "添加自定义 ISO 与 Windows")
        .await
        .unwrap();
    assert!(apply_res.success);

    // 3. 验证底层脚本文件确实被生成
    let script_content = fs::read_to_string(&custom_script_path).unwrap();
    assert!(script_content.contains("Ubuntu Live ISO"));
    assert!(script_content.contains("Windows 11"));

    // 4. 重新查询验证
    let reloaded_custom = proxy.get_custom_entries().await.unwrap();
    assert_eq!(reloaded_custom.len(), 2);
    assert_eq!(reloaded_custom[0].id, "ubuntu_iso");
    assert_eq!(reloaded_custom[1].id, "win11");

    // 5. 设置条目别名
    proxy
        .set_entry_alias("gnulinux-6.8", "Ubuntu 6.8 (生产环境)")
        .await
        .unwrap();

    // 6. 获取所有别名并验证
    let aliases = proxy.get_entry_aliases().await.unwrap();
    assert_eq!(
        aliases.get("gnulinux-6.8").unwrap(),
        "Ubuntu 6.8 (生产环境)"
    );
}
