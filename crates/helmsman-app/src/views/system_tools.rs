use crate::app::HelmsmanWindow;
use crate::views::{page_header, scroll_page_container, section_card, setting_row};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::label::Label;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// 渲染“系统救援与脚本”页面（`/etc/grub.d/` 脚本管理、`grub-install` 重装引导）
pub fn render_system_tools(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    scroll_page_container()
        .child(page_header(
            "系统救援与脚本管理",
            "管理 /etc/grub.d/ 原生脚本以及重装 GRUB 引导记录",
            Some(
                Tag::new()
                    .with_variant(TagVariant::Warning)
                    .small()
                    .child("高危系统工具")
                    .into_any_element(),
            ),
            cx,
        ))
        .child(render_grub_scripts_section(win, cx))
        .child(render_reinstall_grub_section(win, cx))
}

fn render_grub_scripts_section(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let selected_name = win
        .selected_script_name
        .as_deref()
        .or_else(|| win.backend.grub_scripts.first().map(|s| s.name.as_str()));
    let selected_script =
        selected_name.and_then(|name| win.backend.grub_scripts.iter().find(|s| s.name == name));

    section_card("/etc/grub.d/ 原生引导脚本", cx)
        .child(
            Label::new("通过开关控制脚本可执行权限（chmod +x / -x），不篡改发行版脚本正文")
                .text_xs()
                .text_color(cx.theme().muted_foreground),
        )
        .child(div().flex().flex_col().gap_1p5().children(
            win.backend.grub_scripts.iter().enumerate().map(|(idx, s)| {
                let is_selected = selected_name == Some(s.name.as_str());
                let name_select = s.name.clone();
                let name_toggle = s.name.clone();
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .border_1()
                    .border_color(if is_selected {
                        cx.theme().primary
                    } else {
                        cx.theme().border.opacity(0.6)
                    })
                    .bg(if is_selected {
                        cx.theme().primary.opacity(0.08)
                    } else {
                        cx.theme().background
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(Label::new(s.name.clone()).text_xs().font_medium())
                            .child(
                                Tag::new()
                                    .with_variant(if s.enabled {
                                        TagVariant::Success
                                    } else {
                                        TagVariant::Secondary
                                    })
                                    .xsmall()
                                    .child(if s.enabled { "已启用" } else { "已禁用" }),
                            )
                            .when(s.is_helmsman_managed, |el| {
                                el.child(
                                    Tag::new()
                                        .with_variant(TagVariant::Info)
                                        .xsmall()
                                        .child("Helmsman 托管"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new(("btn-view-script", idx))
                                    .ghost()
                                    .xsmall()
                                    .label("查看源码")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.selected_script_name = Some(name_select.clone());
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Switch::new(("sw-script-enabled", idx))
                                    .checked(s.enabled)
                                    .on_click(cx.listener(move |this, checked, _, cx| {
                                        this.toggle_grub_script_action(&name_toggle, *checked, cx);
                                    })),
                            ),
                    )
            }),
        ))
        .when_some(selected_script, |card, script| {
            card.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .mt_2()
                    .child(
                        Label::new(format!(
                            "脚本源码预览：{} ({} 字节)",
                            script.name, script.size_bytes
                        ))
                        .text_xs()
                        .font_semibold(),
                    )
                    .child(
                        div()
                            .max_h(px(200.0))
                            .overflow_y_scrollbar()
                            .p_3()
                            .rounded_md()
                            .border_1()
                            .border_color(cx.theme().border)
                            .bg(cx.theme().secondary.opacity(0.4))
                            .child(Label::new(script.content.clone()).text_xs()),
                    ),
            )
        })
}

fn render_reinstall_grub_section(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let is_uefi = win.reinstall_mode_uefi;

    section_card("重装 GRUB 引导程序 (grub-install)", cx)
        .child(
            Label::new("警告：错误的磁盘设备或 EFI 分区路径可能导致系统无法引导，请核对下方块设备列表后再执行。")
                .text_xs()
                .text_color(cx.theme().danger),
        )
        .child(setting_row(
            "引导安装模式",
            "根据主板固件选择 UEFI（写入 ESP 分区）或传统 BIOS（写入磁盘 MBR）",
            div()
                .flex()
                .items_center()
                .gap_2()
                .child({
                    let b = Button::new("btn-mode-uefi").xsmall().label("UEFI (ESP)");
                    if is_uefi { b.primary() } else { b.outline() }.on_click(cx.listener(
                        |this, _, _, cx| {
                            this.reinstall_mode_uefi = true;
                            cx.notify();
                        },
                    ))
                })
                .child({
                    let b = Button::new("btn-mode-bios").xsmall().label("BIOS (MBR)");
                    if !is_uefi { b.primary() } else { b.outline() }.on_click(cx.listener(
                        |this, _, _, cx| {
                            this.reinstall_mode_uefi = false;
                            cx.notify();
                        },
                    ))
                }),
            cx,
        ))
        .child(if is_uefi {
            setting_row(
                "EFI 系统分区挂载目录 (--efi-directory)",
                "通常为 /boot/efi，下方可点击已挂载的 vfat 分区快速填充",
                div()
                    .w(px(260.0))
                    .child(Input::new(&win.extra_inputs.install_efi_input).small()),
                cx,
            )
        } else {
            setting_row(
                "目标磁盘设备 (/dev/...)",
                "请选择整块物理磁盘（如 /dev/sda），切勿填入分区编号",
                div()
                    .w(px(260.0))
                    .child(Input::new(&win.extra_inputs.install_device_input).small()),
                cx,
            )
        })
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_1p5()
                .children(win.backend.block_devices.iter().enumerate().map(|(idx, d)| {
                    let dev_path = d.path.clone();
                    let mount = d.mountpoint.clone();
                    let is_efi_cand = d.device_type == "part" && !mount.is_empty();
                    let label = if d.mountpoint.is_empty() {
                        format!("{} ({}) {}", d.path, d.size, d.model)
                    } else {
                        format!("{} -> {} ({})", d.path, d.mountpoint, d.fstype)
                    };
                    Button::new(("btn-pick-dev", idx))
                        .outline()
                        .xsmall()
                        .label(label)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.reinstall_mode_uefi && is_efi_cand {
                                this.extra_inputs
                                    .install_efi_input
                                    .update(cx, |s, cx| s.set_value(&mount, window, cx));
                            } else {
                                this.extra_inputs
                                    .install_device_input
                                    .update(cx, |s, cx| s.set_value(&dev_path, window, cx));
                            }
                            cx.notify();
                        }))
                })),
        )
        .child(
            div().flex().justify_end().pt_2().child(
                Button::new("btn-exec-grub-install")
                    .danger()
                    .small()
                    .icon(IconName::Check)
                    .label("执行重装 GRUB 引导程序")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.perform_reinstall_grub(cx);
                    })),
            ),
        )
}
