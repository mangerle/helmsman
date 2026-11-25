use crate::app::HelmsmanWindow;
use crate::views::{kv_row, section_card};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::label::Label;
use gpui_kit::component::{ActiveTheme as _, Sizable as _};
use gpui_kit::*;
use helmsman_ui::{FontScale, ThemeMode};

/// 渲染“设置”页面（界面视觉主题与字体缩放）
pub fn render_settings(win: &HelmsmanWindow, cx: &mut Context<HelmsmanWindow>) -> impl IntoElement {
    let current_theme = win.state.get_theme_mode();
    let current_scale = win.state.get_font_scale();

    div()
        .flex()
        .flex_col()
        .size_full()
        .p_4()
        .gap_4()
        .child(
            section_card("界面视觉主题", cx)
                .child(
                    Label::new("切换桌面应用自身的外观配色，不会影响系统 GRUB 开机主题。")
                        .text_xs()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div().flex().gap_2().children(
                        [
                            (ThemeMode::Dark, "深色 (Dark)"),
                            (ThemeMode::Light, "浅色 (Light)"),
                            (ThemeMode::HighContrast, "高对比度"),
                            (ThemeMode::System, "跟随系统"),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(idx, (mode, title))| {
                            let btn = Button::new(("ui-theme", idx)).small().label(title);
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
                ),
        )
        .child(
            section_card("界面字体缩放", cx).child(
                div().flex().gap_2().children(
                    [
                        (FontScale::Standard, "标准 (1.0x)"),
                        (FontScale::Large, "大号 (1.2x)"),
                        (FontScale::ExtraLarge, "特大 (1.4x)"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(idx, (scale, title))| {
                        let btn = Button::new(("font-scale", idx)).small().label(title);
                        let btn = if current_scale == scale {
                            btn.primary()
                        } else {
                            btn.outline()
                        };
                        btn.on_click(cx.listener(move |this, _, _, cx| {
                            this.state.set_font_scale(scale);
                            cx.notify();
                        }))
                    }),
                ),
            ),
        )
}

/// 渲染“关于与系统状态”页面
pub fn render_about(win: &HelmsmanWindow, cx: &mut Context<HelmsmanWindow>) -> impl IntoElement {
    let status = win.backend.system_status.as_ref();
    div()
        .flex()
        .flex_col()
        .size_full()
        .p_4()
        .gap_4()
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
