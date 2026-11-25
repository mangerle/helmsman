use crate::app::HelmsmanWindow;
use crate::views::{kv_row, section_card};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::label::Label;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use grub_boot_reader::CustomBootEntry;
use grub_config_parser::well_known_flags;
use helmsman_ui::BootEntryItem;

/// 渲染“启动项”主页面（左侧引导列表 + 右侧属性检查器与内核开关）
pub fn render_boot_entries(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let items = win.state.get_entry_items();
    let selected_path = win.state.selected_entry_path.clone();

    div()
        .flex()
        .size_full()
        .min_h_0()
        .child(render_entry_list_pane(
            win,
            &items,
            selected_path.as_deref(),
            cx,
        ))
        .child(render_inspector_pane(win, cx))
}

fn render_entry_list_pane(
    win: &HelmsmanWindow,
    items: &[BootEntryItem],
    selected_path: Option<&str>,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .w(px(380.0))
        .h_full()
        .border_r_1()
        .border_color(cx.theme().border)
        .p_3()
        .gap_2()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    Label::new(format!("引导菜单项 ({})", items.len()))
                        .text_sm()
                        .font_semibold(),
                )
                .child(
                    Button::new("btn-add-sample-custom")
                        .outline()
                        .xsmall()
                        .label("+ 添加 ISO 条目")
                        .on_click(cx.listener(|this, _, _, cx| {
                            let idx = this.state.draft_custom_entries.len() + 1;
                            let entry = CustomBootEntry::new_iso_boot(
                                format!("custom_iso_{idx}"),
                                format!("自定义 Live ISO #{idx}"),
                                "/boot/iso/live.iso",
                                "0000-0000",
                                "",
                            );
                            this.state.add_custom_entry(entry);
                            this.toast = Some((
                                true,
                                format!("已添加自定义 ISO 草稿条目 #{idx}（提交保存后生效）"),
                            ));
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap_1p5()
                .overflow_hidden()
                .children(items.iter().enumerate().map(|(idx, item)| {
                    let is_selected = selected_path == Some(item.full_path.as_str());
                    let full_path = item.full_path.clone();
                    let entry_id = item.id.clone();
                    let is_custom = item.is_custom;

                    div()
                        .id(("boot-entry-row", idx))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .p_2p5()
                        .rounded_md()
                        .border_1()
                        .border_color(if is_selected {
                            cx.theme().primary
                        } else {
                            cx.theme().border
                        })
                        .bg(if is_selected {
                            cx.theme().primary.opacity(0.12)
                        } else {
                            cx.theme().secondary.opacity(0.3)
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.select_entry(&full_path);
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .flex_1()
                                .gap_0p5()
                                .child(
                                    Label::new(item.display_title.clone())
                                        .text_xs()
                                        .font_medium(),
                                )
                                .child(
                                    Label::new(item.full_path.clone())
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .when(item.is_default, |el| {
                                    el.child(
                                        Tag::new()
                                            .with_variant(TagVariant::Primary)
                                            .xsmall()
                                            .child("默认"),
                                    )
                                })
                                .when(is_custom, |el| {
                                    el.child(
                                        Tag::new()
                                            .with_variant(TagVariant::Info)
                                            .xsmall()
                                            .child("自定义"),
                                    )
                                })
                                .when(is_custom, |el| {
                                    let del_id = entry_id.clone().unwrap_or_default();
                                    el.child(
                                        Button::new(("del-custom", idx))
                                            .danger()
                                            .xsmall()
                                            .label("删除")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                if this.state.remove_custom_entry(&del_id) {
                                                    this.toast = Some((
                                                        true,
                                                        format!("已移除自定义条目 '{del_id}'"),
                                                    ));
                                                }
                                                cx.notify();
                                            })),
                                    )
                                }),
                        )
                })),
        )
        .child(
            Label::new(format!(
                "当前 GRUB_DEFAULT = {}",
                win.state.get_default_entry().unwrap_or("0")
            ))
            .text_xs()
            .text_color(cx.theme().muted_foreground),
        )
}

fn render_inspector_pane(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let selected = win.state.get_selected_entry().cloned();
    let cmdline_default = win.state.get_cmdline_default();
    let has_quiet = cmdline_default.split_whitespace().any(|t| t == "quiet");
    let has_splash = cmdline_default.split_whitespace().any(|t| t == "splash");
    let has_nomodeset = cmdline_default.split_whitespace().any(|t| t == "nomodeset");
    let recovery_disabled = win.state.is_recovery_disabled();
    let submenu_disabled = win.state.is_submenu_disabled();
    let os_prober_disabled = win.state.get_menu_visibility().os_prober_disabled;

    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .h_full()
        .p_4()
        .gap_4()
        .child(match selected {
            Some(entry) => {
                let path_for_default = entry.full_path.clone();
                section_card("选中条目属性", cx)
                    .child(kv_row("显示标题", &entry.title, cx))
                    .child(kv_row("菜单路径", &entry.full_path, cx))
                    .child(kv_row(
                        "条目 ID",
                        entry.id.as_deref().unwrap_or("未声明"),
                        cx,
                    ))
                    .child(kv_row(
                        "内核镜像",
                        entry
                            .kernel_path
                            .as_deref()
                            .unwrap_or("无（链式/非 Linux）"),
                        cx,
                    ))
                    .child(kv_row(
                        "Initrd",
                        entry.initrd_path.as_deref().unwrap_or("无"),
                        cx,
                    ))
                    .child(kv_row(
                        "根分区 UUID",
                        entry.root_uuid.as_deref().unwrap_or("未知"),
                        cx,
                    ))
                    .child(
                        div().flex().gap_2().pt_2().child(
                            Button::new("btn-set-default")
                                .primary()
                                .small()
                                .label("设为默认启动项（原生语义索引）")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    match this.state.set_default_entry_semantic(&path_for_default) {
                                        Ok(()) => {
                                            this.toast = Some((
                                                true,
                                                format!(
                                                    "已将 '{path_for_default}' 设为默认启动项草稿"
                                                ),
                                            ));
                                        }
                                        Err(e) => {
                                            this.toast = Some((false, e.to_string()));
                                        }
                                    }
                                    cx.notify();
                                })),
                        ),
                    )
            }
            None => section_card("选中条目属性", cx).child(
                Label::new("请在左侧列表选择一个引导条目以查看详情。")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground),
            ),
        })
        .child(
            section_card("全局内核启动参数 (GRUB_CMDLINE_LINUX_DEFAULT)", cx)
                .child(
                    Label::new(if cmdline_default.is_empty() {
                        "当前为空"
                    } else {
                        cmdline_default
                    })
                    .text_xs()
                    .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_4()
                        .child(
                            Switch::new("sw-quiet")
                                .checked(has_quiet)
                                .label("安静启动 (quiet)")
                                .on_click(cx.listener(|this, checked, _, cx| {
                                    let _ = this.state.set_cmdline_default_flag(
                                        well_known_flags::QUIET,
                                        *checked,
                                    );
                                    cx.notify();
                                })),
                        )
                        .child(
                            Switch::new("sw-splash")
                                .checked(has_splash)
                                .label("开机画面 (splash)")
                                .on_click(cx.listener(|this, checked, _, cx| {
                                    let _ = this.state.set_cmdline_default_flag(
                                        well_known_flags::SPLASH,
                                        *checked,
                                    );
                                    cx.notify();
                                })),
                        )
                        .child(
                            Switch::new("sw-nomodeset")
                                .checked(has_nomodeset)
                                .label("禁用内核模式设置 (nomodeset)")
                                .on_click(cx.listener(|this, checked, _, cx| {
                                    let _ = this.state.set_cmdline_default_flag(
                                        well_known_flags::NOMODESET,
                                        *checked,
                                    );
                                    cx.notify();
                                })),
                        ),
                ),
        )
        .child(
            section_card("菜单类级可见性策略（零侵入官方开关）", cx).child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .child(
                        Switch::new("sw-recovery")
                            .checked(recovery_disabled)
                            .label("隐藏恢复模式 (GRUB_DISABLE_RECOVERY)")
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.state.set_recovery_disabled(*checked);
                                cx.notify();
                            })),
                    )
                    .child(
                        Switch::new("sw-submenu")
                            .checked(submenu_disabled)
                            .label("拉平二级子菜单 (GRUB_DISABLE_SUBMENU)")
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.state.set_submenu_disabled(*checked);
                                cx.notify();
                            })),
                    )
                    .child(
                        Switch::new("sw-osprober")
                            .checked(!os_prober_disabled)
                            .label("启用多系统探测 (os-prober)")
                            .on_click(cx.listener(|this, checked, _, cx| {
                                this.state.set_os_prober_enabled(*checked);
                                cx.notify();
                            })),
                    ),
            ),
        )
}
