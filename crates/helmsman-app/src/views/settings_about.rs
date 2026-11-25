use crate::app::{AppUpdateStatus, HelmsmanWindow};
use crate::views::{kv_row, page_header, scroll_page_container, section_card, setting_row};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::label::Label;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, IconName, Sizable as _};
use gpui_kit::*;
use helmsman_ui::{FontScale, ThemeMode};

/// 渲染“设置”页面（界面视觉主题与字体缩放）
pub fn render_settings(win: &HelmsmanWindow, cx: &mut Context<HelmsmanWindow>) -> impl IntoElement {
    let current_theme = win.state.get_theme_mode();
    let current_scale = win.state.get_font_scale();

    scroll_page_container()
        .child(page_header(
            "偏好设置",
            "调整 Helmsman 桌面客户端自身的外观配色与界面缩放比例",
            None,
            cx,
        ))
        .child(
            section_card("桌面界面视觉", cx)
                .child(setting_row(
                    "界面主题模式",
                    "仅影响本软件窗口配色，不会修改系统 GRUB 引导主题",
                    div().flex().gap_1p5().children(
                        [
                            (ThemeMode::Dark, "深色"),
                            (ThemeMode::Light, "浅色"),
                            (ThemeMode::HighContrast, "高对比度"),
                            (ThemeMode::System, "跟随系统"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(idx, (mode, title))| {
                            let btn = Button::new(("ui-theme", idx)).xsmall().label(title);
                            let btn = if current_theme == mode {
                                btn.primary()
                            } else {
                                btn.outline()
                            };
                            btn.on_click(cx.listener(move |this, _, window, cx| {
                                this.set_ui_theme(mode, window, cx);
                            }))
                        }),
                    ),
                    cx,
                ))
                .child(setting_row(
                    "界面字体缩放",
                    "适配高分屏或提升阅读舒适度",
                    div().flex().gap_1p5().children(
                        [
                            (FontScale::Standard, "标准 1.0x"),
                            (FontScale::Medium, "中等 1.25x"),
                            (FontScale::Large, "大号 1.5x"),
                            (FontScale::ExtraLarge, "特大 2.0x"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(idx, (scale, title))| {
                            let btn = Button::new(("font-scale", idx)).xsmall().label(title);
                            let btn = if current_scale == scale {
                                btn.primary()
                            } else {
                                btn.outline()
                            };
                            btn.on_click(cx.listener(move |this, _, window, cx| {
                                this.set_font_scale(scale, window, cx);
                            }))
                        }),
                    ),
                    cx,
                )),
        )
}

/// 渲染“关于与系统状态”页面
pub fn render_about(win: &HelmsmanWindow, cx: &mut Context<HelmsmanWindow>) -> impl IntoElement {
    let status = win.backend.system_status.as_ref();

    scroll_page_container()
        .child(page_header(
            "更新与系统信息",
            "查看 Helmsman 版本、应用自更新状态与底层系统引导适配器探测结果",
            None,
            cx,
        ))
        .child(render_update_section(win, cx))
        .child(
            section_card("关于 Helmsman", cx)
                .child(kv_row("应用版本", env!("CARGO_PKG_VERSION"), cx))
                .child(kv_row("图形渲染引擎", "gpui-kit 0.11 (GPU 加速)", cx))
                .child(kv_row(
                    "安全架构",
                    "非特权前端 + Polkit / D-Bus 按需特权守护进程",
                    cx,
                ))
                .child(kv_row(
                    "内存安全策略",
                    "100% 纯安全 Rust (forbid unsafe)",
                    cx,
                )),
        )
        .child(
            section_card("当前系统引导适配器探测结果", cx)
                .child(kv_row(
                    "连接状态",
                    if win.backend.is_demo_mode {
                        "离线演示模式"
                    } else {
                        "已连接 helmsman-daemon"
                    },
                    cx,
                ))
                .child(kv_row(
                    "发行版名称",
                    status.map_or("未知（演示环境）", |s| s.distro_name.as_str()),
                    cx,
                ))
                .child(kv_row(
                    "固件类型",
                    status.map_or("未知", |s| s.firmware_type.as_str()),
                    cx,
                ))
                .child(kv_row(
                    "主配置文件",
                    status.map_or("/etc/default/grub", |s| s.config_path.as_str()),
                    cx,
                ))
                .child(kv_row(
                    "目标引导菜单",
                    status.map_or("/boot/grub/grub.cfg", |s| s.target_boot_path.as_str()),
                    cx,
                ))
                .child(kv_row(
                    "引导重建命令",
                    status.map_or("grub-mkconfig / update-grub", |s| s.update_command.as_str()),
                    cx,
                )),
        )
}

fn render_update_section(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let status_text = match &win.update_status {
        AppUpdateStatus::Idle => {
            format!(
                "当前版本 v{}，点击右侧按钮检查新版本",
                env!("CARGO_PKG_VERSION")
            )
        }
        AppUpdateStatus::Checking => "正在向 GitHub Releases 检查签名更新清单...".to_string(),
        AppUpdateStatus::UpToDate => {
            format!("当前 v{} 已是最新稳定版本", env!("CARGO_PKG_VERSION"))
        }
        AppUpdateStatus::Available(info) => {
            format!(
                "发现新版本 v{}，可立即下载并通过 Ed25519 验签安装",
                info.version()
            )
        }
        AppUpdateStatus::Installing(msg) => msg.clone(),
        AppUpdateStatus::ReadyToRestart(ver) => {
            format!("新版本 v{ver} 安装完成，重启应用即可生效")
        }
        AppUpdateStatus::Error(err) => err.clone(),
    };

    let checking = matches!(
        win.update_status,
        AppUpdateStatus::Checking | AppUpdateStatus::Installing(_)
    );

    let mut actions = div().flex().items_center().gap_2().child(
        Button::new("btn-check-update")
            .outline()
            .small()
            .icon(IconName::Globe)
            .disabled(checking)
            .label(if checking {
                "正在处理..."
            } else {
                "检查更新"
            })
            .on_click(cx.listener(|this, _, _, cx| {
                this.check_app_update(cx);
            })),
    );

    if let AppUpdateStatus::Available(info) = &win.update_status {
        let info_clone = (**info).clone();
        actions = actions.child(
            Button::new("btn-install-update")
                .primary()
                .small()
                .icon(IconName::ArrowDown)
                .label(format!("下载并安装 v{}", info.version()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.install_app_update(info_clone.clone(), cx);
                })),
        );
    } else if let AppUpdateStatus::ReadyToRestart(_) = &win.update_status {
        actions = actions.child(
            Button::new("btn-restart-app")
                .primary()
                .small()
                .label("立即重启应用")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.restart_for_update(cx);
                })),
        );
    }

    section_card("应用自更新 (shipup + Ed25519 验签)", cx)
        .child(
            Label::new(status_text)
                .text_xs()
                .text_color(cx.theme().muted_foreground),
        )
        .child(actions)
}
