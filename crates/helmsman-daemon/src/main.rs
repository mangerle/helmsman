use helmsman_daemon::{GrubService, TransactionOptions};
use std::env;
use std::fs;
use std::process;

fn print_usage() {
    println!("Helmsman (舵手) 特权后台服务 (helmsman-daemon)");
    println!("用法:");
    println!("  helmsman-daemon --status               查看当前系统与引导适配状态");
    println!("  helmsman-daemon --list-snapshots       列出所有可用历史配置快照");
    println!("  helmsman-daemon --preview <file>       比对传入新配置与当前配置的 Diff");
    println!("  helmsman-daemon --apply <file> [reason] 提交新配置（含备份与更新引导）");
    println!("  helmsman-daemon --rollback <id>        回滚至指定的历史快照");
}

fn handle_status(service: &GrubService) {
    println!("操作系统识别: {}", service.distro_profile.name);
    println!("发行版家族: {:?}", service.distro_profile.family);
    println!("固件引导类型: {:?}", service.distro_profile.firmware);
    println!("配置文件路径: {}", service.default_config_path.display());
    println!("目标引导文件: {}", service.distro_profile.config_path);
    println!(
        "引导更新命令: {} {:?}",
        service.distro_profile.update_command, service.distro_profile.command_args
    );
}

fn handle_list_snapshots(service: &GrubService) {
    match service.get_available_snapshots() {
        Ok(snapshots) => {
            if snapshots.is_empty() {
                println!("暂无可用的历史配置快照。");
            } else {
                println!("找到 {} 个历史快照:", snapshots.len());
                for s in snapshots {
                    println!(
                        "- 快照 ID: {} (时间戳: {}) - 原因: {}",
                        s.id, s.timestamp, s.reason
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("获取快照失败: {}", e);
            process::exit(1);
        }
    }
}

fn handle_preview(service: &GrubService, args: &[String]) {
    if args.len() < 3 {
        eprintln!("错误: 请指定待比对的新配置文件路径");
        process::exit(1);
    }
    let new_content = fs::read_to_string(&args[2]).unwrap_or_else(|e| {
        eprintln!("无法读取文件: {}", e);
        process::exit(1);
    });
    match service.preview_diff(&new_content) {
        Ok(report) => {
            if report.has_changes {
                println!(
                    "检测到配置变更 (+{} / -{}):",
                    report.added_lines, report.removed_lines
                );
                println!("{}", report.diff_text);
            } else {
                println!("内容无变化。");
            }
        }
        Err(e) => {
            eprintln!("生成差异失败: {}", e);
            process::exit(1);
        }
    }
}

fn handle_apply(service: &GrubService, args: &[String]) {
    if args.len() < 3 {
        eprintln!("错误: 请指定待写入的新配置文件路径");
        process::exit(1);
    }
    let new_content = fs::read_to_string(&args[2]).unwrap_or_else(|e| {
        eprintln!("无法读取文件: {}", e);
        process::exit(1);
    });
    let reason = if args.len() >= 4 {
        &args[3]
    } else {
        "用户通过图形界面修改配置"
    };

    let options = TransactionOptions::default();
    match service.apply_changes(&new_content, reason, &options) {
        Ok(result) => {
            if result.success {
                println!("事务提交成功！已创建快照: {}", result.snapshot_id);
                println!("引导生成输出:\n{}", result.log_output);
            } else {
                eprintln!("事务执行失败: {:?}", result.error_message);
                eprintln!("控制台日志:\n{}", result.log_output);
                process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("执行事务失败: {}", e);
            process::exit(1);
        }
    }
}

fn handle_rollback(service: &GrubService, args: &[String]) {
    if args.len() < 3 {
        eprintln!("错误: 请指定要回滚的快照 ID");
        process::exit(1);
    }
    let snapshot_id = &args[2];
    match service.rollback_to_snapshot(snapshot_id) {
        Ok(()) => {
            println!("成功回滚至快照: {}", snapshot_id);
        }
        Err(e) => {
            eprintln!("回滚失败: {}", e);
            process::exit(1);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let service = GrubService::new_system_default();

    match args[1].as_str() {
        "--status" => handle_status(&service),
        "--list-snapshots" => handle_list_snapshots(&service),
        "--preview" => handle_preview(&service, &args),
        "--apply" => handle_apply(&service, &args),
        "--rollback" => handle_rollback(&service, &args),
        _ => {
            print_usage();
            process::exit(1);
        }
    }
}
