use crate::app::{ActiveModal, HelmsmanWindow};
use crate::views::entry_list_item::render_boot_entry_row;
use crate::views::entry_toolbar::{
    render_custom_detail_rows, render_custom_title_row, render_entry_action_bar,
    render_hidden_entries_panel,
};
use crate::views::{kv_row, page_header, scroll_page_container, section_card, setting_row};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::label::Label;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tag::{Tag, TagVariant};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, StyledExt as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use helmsman_ui::BootEntryItem;

/// 渲染“启动项”主页面（左侧可滚动引导列表 + 右侧可滚动属性检查器）
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
    let query = win
        .entry_search_input
        .read(cx)
        .value()
        .trim()
        .to_lowercase();
    let filtered_items: Vec<&BootEntryItem> = items
        .iter()
        .filter(|item| {
            if query.is_empty() {
                return true;
            }
            item.display_title.to_lowercase().contains(&query)
                || item.full_path.to_lowercase().contains(&query)
                || item
                    .id
                    .as_deref()
                    .is_some_and(|id| id.to_lowercase().contains(&query))
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .w(px(360.0))
        .h_full()
        .border_r_1()
        .border_color(cx.theme().border.opacity(0.75))
        .bg(cx.theme().secondary.opacity(0.2))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2p5()
                .px_4()
                .py_3()
                .border_b_1()
                .border_color(cx.theme().border.opacity(0.5))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(Label::new("引导菜单").text_sm().font_semibold())
                                .child(
                                    Tag::new()
                                        .with_variant(TagVariant::Secondary)
                                        .xsmall()
                                        .child(format!("{}", filtered_items.len())),
                                ),
                        )
                        .child(
                            Button::new("btn-open-create-wizard")
                                .outline()
                                .xsmall()
                                .icon(IconName::Plus)
                                .label("新建条目")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.active_modal = ActiveModal::CreateCustomEntry;
                                    cx.notify();
                                })),
                        ),
                )
                .child(Input::new(&win.entry_search_input).small())
                .child(render_entry_action_bar(win, cx)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .p_3()
                .gap_2()
                .overflow_y_scrollbar()
                .when(filtered_items.is_empty(), |el| {
                    el.child(
                        div().p_4().child(
                            Label::new("未找到匹配的引导条目")
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        ),
                    )
                })
                .children(render_tree_items(
                    win,
                    &filtered_items,
                    selected_path,
                    !query.is_empty(),
                    cx,
                )),
        )
        .child(render_hidden_entries_panel(win, cx))
        .child(
            div()
                .px_4()
                .py_2()
                .border_t_1()
                .border_color(cx.theme().border.opacity(0.5))
                .child(
                    Label::new(format!(
                        "GRUB_DEFAULT = {}",
                        win.state.get_default_entry().unwrap_or("0")
                    ))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground),
                ),
        )
}

fn render_inspector_pane(
    win: &HelmsmanWindow,
    cx: &mut Context<HelmsmanWindow>,
) -> impl IntoElement {
    let selected = win.state.get_selected_entry().cloned();

    scroll_page_container()
        .flex_1()
        .min_w_0()
        .child(page_header(
            "启动项属性详情",
            "在左侧选择或调整引导条目顺序、子菜单归组与隐藏状态，全部变更暂存于草稿中",
            Some(
                Tag::new()
                    .with_variant(TagVariant::Info)
                    .small()
                    .child("草稿编辑")
                    .into_any_element(),
            ),
            cx,
        ))
        .child(match selected {
            Some(entry) => {
                let path_for_default = entry.full_path.clone();
                let is_selected_custom = entry
                    .id
                    .as_deref()
                    .is_some_and(|id| win.state.draft_custom_entries.iter().any(|c| c.id == id));
                section_card("选中条目详情", cx)
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
                    .child(render_custom_title_row(win, cx))
                    .child(setting_row(
                        "友好显示别名",
                        "仅作用于 Helmsman 列表显示，不改动系统内核配置",
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .w(px(280.0))
                            .child(div().flex_1().child(Input::new(&win.alias_input).small()))
                            .child(
                                Button::new("btn-save-alias")
                                    .outline()
                                    .xsmall()
                                    .label("保存")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.save_selected_entry_alias(cx);
                                    })),
                            ),
                        cx,
                    ))
                    .when(is_selected_custom, |el| {
                        el.child(render_custom_detail_rows(win, cx))
                    })
                    .child(
                        div().flex().justify_end().pt_2().child(
                            Button::new("btn-set-default")
                                .primary()
                                .small()
                                .icon(IconName::Check)
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
            None => section_card("选中条目详情", cx).child(
                Label::new("请在左侧列表选择一个引导条目以查看详情。")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground),
            ),
        })
        .child(
            section_card("提示", cx).child(
                Label::new("提示：支持在左侧列表直接鼠标拖拽调整顺序、右键打开快捷菜单、点击子菜单折叠/展开，以及键盘 ↑/↓/Delete/Ctrl+Z/Ctrl+S 快捷操作。")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground),
            ),
        )
}

/// 按树形可折叠子菜单组织渲染左侧引导条目列表
fn render_tree_items(
    win: &HelmsmanWindow,
    items: &[&BootEntryItem],
    selected_path: Option<&str>,
    is_searching: bool,
    cx: &mut Context<HelmsmanWindow>,
) -> Vec<AnyElement> {
    let mut elements = Vec::with_capacity(items.len() + 4);
    let mut rendered_submenus: Vec<String> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        match &item.submenu_name {
            None => {
                elements.push(
                    render_boot_entry_row(win, idx, item, selected_path, false, cx)
                        .into_any_element(),
                );
            }
            Some(sub_name) if is_searching => {
                elements.push(
                    render_boot_entry_row(win, idx, item, selected_path, false, cx)
                        .into_any_element(),
                );
                let _ = sub_name;
            }
            Some(sub_name) => {
                if rendered_submenus.contains(sub_name) {
                    continue;
                }
                rendered_submenus.push(sub_name.clone());
                let is_collapsed = win.collapsed_submenus.contains(sub_name);
                let member_count = items
                    .iter()
                    .filter(|it| it.submenu_name.as_deref() == Some(sub_name.as_str()))
                    .count();
                let sub_toggle = sub_name.clone();

                elements.push(
                    div()
                        .id(("submenu-group", idx))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2p5()
                        .py_1p5()
                        .rounded_lg()
                        .bg(cx.theme().secondary.opacity(0.45))
                        .hover(|s| s.bg(cx.theme().secondary.opacity(0.7)))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.collapsed_submenus.remove(&sub_toggle) {
                                this.collapsed_submenus.insert(sub_toggle.clone());
                            }
                            cx.notify();
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1p5()
                                .child(if is_collapsed {
                                    IconName::ChevronRight
                                } else {
                                    IconName::ChevronDown
                                })
                                .child(if is_collapsed {
                                    IconName::Folder
                                } else {
                                    IconName::FolderOpen
                                })
                                .child(Label::new(sub_name.clone()).text_xs().font_semibold()),
                        )
                        .child(
                            Tag::new()
                                .with_variant(TagVariant::Secondary)
                                .xsmall()
                                .child(format!("{member_count}")),
                        )
                        .into_any_element(),
                );

                if !is_collapsed {
                    for (m_idx, member) in items.iter().enumerate() {
                        if member.submenu_name.as_deref() == Some(sub_name.as_str()) {
                            elements.push(
                                render_boot_entry_row(
                                    win,
                                    1000 + m_idx,
                                    member,
                                    selected_path,
                                    true,
                                    cx,
                                )
                                .into_any_element(),
                            );
                        }
                    }
                }
            }
        }
    }
    elements
}
