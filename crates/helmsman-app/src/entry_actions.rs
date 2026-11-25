use crate::app::{ActiveModal, ActiveTab, HelmsmanWindow};
use crate::backend;
use gpui_kit::*;
use tracing::warn;

impl HelmsmanWindow {
    /// 选中启动条目并同步刷新别名、标题与自定义属性输入框内容
    pub fn select_boot_entry(
        &mut self,
        full_path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.select_entry(full_path);
        let alias = self
            .state
            .get_selected_entry()
            .and_then(|e| {
                e.id.as_deref()
                    .and_then(|id| self.state.get_entry_alias(id))
                    .or_else(|| self.state.get_entry_alias(&e.title))
            })
            .unwrap_or("")
            .to_string();
        self.alias_input.update(cx, |input, cx| {
            input.set_value(alias, window, cx);
        });
        let menu_title = self
            .state
            .get_selected_entry()
            .map(|e| {
                self.state
                    .get_entry_title_override(&e.full_path)
                    .map(str::to_string)
                    .unwrap_or_else(|| e.title.clone())
            })
            .unwrap_or_default();
        self.custom_title_input.update(cx, |input, cx| {
            input.set_value(menu_title, window, cx);
        });
        let selected_custom = self
            .state
            .get_selected_entry()
            .and_then(|e| e.id.as_deref())
            .and_then(|id| {
                self.state
                    .draft_custom_entries
                    .iter()
                    .find(|c| c.id == id)
                    .cloned()
            });
        self.extra_inputs
            .sync_selected_custom(selected_custom.as_ref(), window, cx);
        cx.notify();
    }

    /// 保存当前选中自定义条目的完整引导属性（路径、UUID、参数与子菜单名称）
    pub fn save_selected_custom_details(&mut self, cx: &mut Context<Self>) {
        let Some(entry_id) = self.state.get_selected_entry().and_then(|e| e.id.clone()) else {
            return;
        };
        let Some(mut custom) = self
            .state
            .draft_custom_entries
            .iter()
            .find(|c| c.id == entry_id)
            .cloned()
        else {
            return;
        };
        self.state.push_undo_snapshot();
        custom.target_path = self
            .extra_inputs
            .custom_target_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        custom.partition_uuid = self
            .extra_inputs
            .custom_uuid_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        custom.cmdline_params = self
            .extra_inputs
            .custom_cmdline_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        custom.submenu_title = self
            .extra_inputs
            .custom_submenu_input
            .read(cx)
            .value()
            .trim()
            .to_string();

        if self.state.update_custom_entry(custom) {
            self.toast = Some((
                true,
                "已更新自定义引导条目属性草稿（审核保存后生效）".to_string(),
            ));
        }
        cx.notify();
    }

    /// 保存当前选中引导条目（系统条目或自定义条目）在 GRUB 开机菜单中的显示名称
    pub fn save_selected_entry_title(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.state.get_selected_entry().cloned() else {
            return;
        };
        let new_title = self.custom_title_input.read(cx).value().trim().to_string();

        if let Some(entry_id) = entry.id.as_deref()
            && let Some(mut custom) = self
                .state
                .draft_custom_entries
                .iter()
                .find(|c| c.id == entry_id)
                .cloned()
        {
            if new_title.is_empty() {
                self.toast = Some((false, "自定义条目显示名称不能为空".to_string()));
                cx.notify();
                return;
            }
            self.state.push_undo_snapshot();
            custom.title = new_title.clone();
            if self.state.update_custom_entry(custom) {
                self.state.select_entry(&new_title);
                self.toast = Some((
                    true,
                    format!("已将 GRUB 菜单显示名称更新为 '{new_title}'（审核保存后生效）"),
                ));
            }
            cx.notify();
            return;
        }

        self.state.push_undo_snapshot();
        if self.state.rename_entry(&entry.full_path, &new_title) {
            let msg = if new_title.is_empty() {
                format!("已恢复 '{}' 的原始 GRUB 菜单名称", entry.title)
            } else {
                format!("已将 GRUB 菜单显示名称修改为 '{new_title}'（审核保存后生效）")
            };
            self.toast = Some((true, msg));
        }
        cx.notify();
    }

    /// 保存当前选中启动项的自定义友好别名
    pub fn save_selected_entry_alias(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.state.get_selected_entry().cloned() else {
            return;
        };
        let key = entry.id.unwrap_or_else(|| entry.title.clone());
        let alias = self.alias_input.read(cx).value().trim().to_string();

        if !self.backend.is_demo_mode
            && let Err(e) = backend::set_entry_alias(&key, &alias)
        {
            warn!("保存引导项别名失败: {e}");
            self.toast = Some((false, e));
            cx.notify();
            return;
        }

        self.state.set_entry_alias(&key, &alias);
        let msg = if alias.is_empty() {
            format!("已清除条目 '{key}' 的自定义别名")
        } else {
            format!("已将条目别名更新为 '{alias}'")
        };
        self.toast = Some((true, msg));
        cx.notify();
    }

    /// 撤销最近一次草稿操作（Ctrl+Z 或工具栏撤销按钮）
    pub fn perform_undo(&mut self, cx: &mut Context<Self>) {
        if self.state.undo() {
            self.toast = Some((true, "已撤销上一步操作".to_string()));
        } else {
            self.toast = Some((false, "当前没有可撤销的操作".to_string()));
        }
        cx.notify();
    }

    /// 提交“新建自定义引导条目”向导表单并加入草稿
    pub fn submit_create_wizard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self
            .create_wizard
            .title_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        if title.is_empty() {
            self.toast = Some((false, "请填写菜单显示标题".to_string()));
            cx.notify();
            return;
        }
        let idx = self.state.draft_custom_entries.len() + 1;
        let id = format!("custom_{idx}_{}", title.len());
        let entry = self.create_wizard.build_entry(id, cx);
        let path = entry.title.clone();

        self.state.push_undo_snapshot();
        self.state.add_custom_entry(entry);
        self.active_modal = ActiveModal::None;
        self.select_boot_entry(&path, window, cx);
        self.toast = Some((
            true,
            format!("已创建自定义引导条目 '{title}'（审核保存后生效）"),
        ));
        cx.notify();
    }

    /// 统一处理窗口级键盘快捷键（Ctrl+S / Ctrl+Z / Delete / 上下方向键 / Esc）
    pub fn handle_key_down(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = ev.keystroke.key.as_str();
        let ctrl = ev.keystroke.modifiers.control;

        if ctrl && key.eq_ignore_ascii_case("s") {
            self.open_apply_modal(cx);
            return;
        }
        if ctrl && key.eq_ignore_ascii_case("z") {
            self.perform_undo(cx);
            return;
        }
        if key == "escape" && self.active_modal != ActiveModal::None {
            self.active_modal = ActiveModal::None;
            cx.notify();
            return;
        }

        if self.active_modal != ActiveModal::None || self.active_tab != ActiveTab::BootEntries {
            return;
        }

        let items = self.state.get_entry_items();
        if items.is_empty() {
            return;
        }
        let current_idx = self
            .state
            .selected_entry_path
            .as_deref()
            .and_then(|p| items.iter().position(|item| item.full_path == p));

        match key {
            "up" => {
                let next = current_idx.map_or(0, |i| i.saturating_sub(1));
                let path = items[next].full_path.clone();
                self.select_boot_entry(&path, window, cx);
            }
            "down" => {
                let next = current_idx.map_or(0, |i| (i + 1).min(items.len() - 1));
                let path = items[next].full_path.clone();
                self.select_boot_entry(&path, window, cx);
            }
            "delete" => {
                if let Some(idx) = current_idx {
                    let path = items[idx].full_path.clone();
                    self.state.push_undo_snapshot();
                    if self.state.hide_entry(&path) {
                        self.toast = Some((
                            true,
                            format!("已移除条目 '{path}'（可按 Ctrl+Z 撤销或下方恢复）"),
                        ));
                        cx.notify();
                    }
                }
            }
            _ => {}
        }
    }
}
