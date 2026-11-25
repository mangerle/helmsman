use super::AppState;
use crate::entry_view::{BootEntryItem, flatten_boot_entries_with_options};

impl AppState {
    /// 检查菜单编排布局（顺序、隐藏、子菜单归组）是否有未保存变更
    pub fn has_menu_layout_changes(&self) -> bool {
        self.original_menu_layout != self.draft_menu_layout
    }

    /// 收集当前所有未经隐藏过滤的原始条目列表
    pub(super) fn collect_unfiltered_items(&self) -> Vec<BootEntryItem> {
        let filter_recovery = self.is_recovery_disabled();
        let default_entry = self.get_default_entry();
        let mut items = flatten_boot_entries_with_options(
            &self.menu_nodes,
            default_entry,
            &self.aliases,
            filter_recovery,
        );

        for custom in &self.draft_custom_entries {
            if custom.enabled {
                items.push(BootEntryItem::from_custom_entry(custom, default_entry));
            }
        }

        items
    }

    /// 确保 `draft_menu_layout.order` 包含当前所有可见条目的顺序基准
    fn ensure_order_initialized(&mut self) {
        let current_items = self.get_entry_items();
        let mut order = Vec::with_capacity(current_items.len());
        for key in &self.draft_menu_layout.order {
            if current_items.iter().any(|item| &item.full_path == key) && !order.contains(key) {
                order.push(key.clone());
            }
        }
        for item in current_items {
            if !order.contains(&item.full_path) {
                order.push(item.full_path);
            }
        }
        self.draft_menu_layout.order = order;
    }

    /// 将任意引导条目（系统条目或自定义条目）在当前可见列表中上移一位
    pub fn move_entry_up(&mut self, full_path: &str) -> bool {
        self.ensure_order_initialized();
        if let Some(pos) = self
            .draft_menu_layout
            .order
            .iter()
            .position(|k| k == full_path)
            && pos > 0
        {
            self.draft_menu_layout.order.swap(pos - 1, pos);
            return true;
        }
        false
    }

    /// 将任意引导条目（系统条目或自定义条目）在当前可见列表中下移一位
    pub fn move_entry_down(&mut self, full_path: &str) -> bool {
        self.ensure_order_initialized();
        if let Some(pos) = self
            .draft_menu_layout
            .order
            .iter()
            .position(|k| k == full_path)
            && pos + 1 < self.draft_menu_layout.order.len()
        {
            self.draft_menu_layout.order.swap(pos, pos + 1);
            return true;
        }
        false
    }

    /// 将任意引导条目直接拖拽移动至目标条目的位置
    pub fn move_entry_to(&mut self, source_path: &str, target_path: &str) -> bool {
        if source_path == target_path {
            return false;
        }
        self.ensure_order_initialized();
        let Some(src_idx) = self
            .draft_menu_layout
            .order
            .iter()
            .position(|k| k == source_path)
        else {
            return false;
        };
        let Some(dst_idx) = self
            .draft_menu_layout
            .order
            .iter()
            .position(|k| k == target_path)
        else {
            return false;
        };
        let item = self.draft_menu_layout.order.remove(src_idx);
        self.draft_menu_layout.order.insert(dst_idx, item);
        true
    }

    /// 移除（隐藏）任意引导条目，使其暂不出现在主引导菜单中，并支持单条恢复
    pub fn hide_entry(&mut self, full_path: &str) -> bool {
        if full_path.is_empty() || self.draft_menu_layout.hidden.iter().any(|h| h == full_path) {
            return false;
        }
        if !self
            .collect_unfiltered_items()
            .iter()
            .any(|item| item.full_path == full_path)
        {
            return false;
        }
        self.draft_menu_layout.hidden.push(full_path.to_string());
        if self.selected_entry_path.as_deref() == Some(full_path) {
            self.selected_entry_path = self
                .get_entry_items()
                .first()
                .map(|item| item.full_path.clone());
        }
        true
    }

    /// 恢复已被移除（隐藏）的指定引导条目
    pub fn restore_entry(&mut self, full_path: &str) -> bool {
        let prev = self.draft_menu_layout.hidden.len();
        self.draft_menu_layout.hidden.retain(|h| h != full_path);
        self.draft_menu_layout.hidden.len() < prev
    }

    /// 列出当前已被移除（隐藏）的引导条目
    pub fn list_hidden_entries(&self) -> Vec<BootEntryItem> {
        self.collect_unfiltered_items()
            .into_iter()
            .filter(|item| self.draft_menu_layout.hidden.contains(&item.full_path))
            .collect()
    }

    /// 判断指定条目当前是否位于子菜单中（综合原始路径与用户覆写）
    pub fn is_entry_in_submenu(&self, full_path: &str) -> bool {
        if let Some(override_sub) = self.draft_menu_layout.submenu_overrides.get(full_path) {
            return !override_sub.trim().is_empty();
        }
        self.collect_unfiltered_items()
            .into_iter()
            .find(|item| item.full_path == full_path)
            .is_some_and(|item| item.submenu_name.is_some())
    }

    /// 将任意引导条目归组移入指定子菜单（空标题等价于移回根层级）
    pub fn move_entry_to_submenu(&mut self, full_path: &str, submenu_title: &str) -> bool {
        if !self
            .collect_unfiltered_items()
            .iter()
            .any(|item| item.full_path == full_path)
        {
            return false;
        }
        let target = submenu_title.trim().to_string();
        self.draft_menu_layout
            .submenu_overrides
            .insert(full_path.to_string(), target);
        true
    }

    /// 将任意引导条目从子菜单移回主菜单根层级
    pub fn move_entry_to_root(&mut self, full_path: &str) -> bool {
        self.move_entry_to_submenu(full_path, "")
    }

    /// 修改任意引导条目（系统条目或自定义条目）在 GRUB 开机菜单中的显示名称（留空恢复原标题）
    pub fn rename_entry(&mut self, full_path: &str, new_title: &str) -> bool {
        if !self
            .collect_unfiltered_items()
            .iter()
            .any(|item| item.full_path == full_path)
        {
            return false;
        }
        let trimmed = new_title.trim();
        if trimmed.is_empty() {
            self.draft_menu_layout.title_overrides.remove(full_path);
        } else {
            self.draft_menu_layout
                .title_overrides
                .insert(full_path.to_string(), trimmed.to_string());
        }
        true
    }

    /// 获取条目被覆写后的 GRUB 菜单显示名称（未覆写返回 `None`）
    pub fn get_entry_title_override(&self, full_path: &str) -> Option<&str> {
        self.draft_menu_layout
            .title_overrides
            .get(full_path)
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO_CFG: &str = r#"
menuentry 'Ubuntu 24.04 LTS' --id 'gnulinux-ubuntu' {
    linux /vmlinuz
}
submenu 'Advanced options for Ubuntu' {
    menuentry 'Ubuntu, with Linux 6.8.0' --id 'gnulinux-6.8.0' {
        linux /vmlinuz-6.8.0
    }
}
menuentry 'Windows Boot Manager' --id 'win-efi' {
    chainloader /efi/bootmgfw.efi
}
"#;

    #[test]
    fn test_universal_entry_order_hide_restore_and_submenu() {
        let mut state = AppState::new_from_content("", DEMO_CFG);

        // 1. 系统条目上移/下移
        assert!(state.move_entry_down("Ubuntu 24.04 LTS"));
        let items = state.get_entry_items();
        assert_eq!(items[1].full_path, "Ubuntu 24.04 LTS");

        assert!(state.move_entry_up("Ubuntu 24.04 LTS"));
        assert_eq!(state.get_entry_items()[0].full_path, "Ubuntu 24.04 LTS");

        // 2. 子菜单移出与移入
        assert!(state.is_entry_in_submenu("Advanced options for Ubuntu>Ubuntu, with Linux 6.8.0"));
        assert!(state.move_entry_to_root("Advanced options for Ubuntu>Ubuntu, with Linux 6.8.0"));
        assert!(!state.is_entry_in_submenu("Advanced options for Ubuntu>Ubuntu, with Linux 6.8.0"));
        assert!(state.move_entry_to_submenu("Windows Boot Manager", "其他系统"));
        assert!(state.is_entry_in_submenu("Windows Boot Manager"));

        // 3. 隐藏与恢复系统条目
        assert!(state.hide_entry("Windows Boot Manager"));
        assert_eq!(state.list_hidden_entries().len(), 1);
        assert!(
            !state
                .get_entry_items()
                .iter()
                .any(|i| i.full_path == "Windows Boot Manager")
        );

        assert!(state.restore_entry("Windows Boot Manager"));
        assert!(state.list_hidden_entries().is_empty());
        assert!(
            state
                .get_entry_items()
                .iter()
                .any(|i| i.full_path == "Windows Boot Manager")
        );

        // 4. 修改系统条目 GRUB 菜单显示名称
        assert!(state.rename_entry("Windows Boot Manager", "Windows 11"));
        let renamed = state
            .get_entry_items()
            .into_iter()
            .find(|i| i.full_path == "Windows Boot Manager");
        assert_eq!(
            renamed.map(|i| i.display_title).as_deref(),
            Some("Windows 11")
        );

        // 5. 任意位置拖拽排序
        assert!(state.move_entry_to("Windows Boot Manager", "Ubuntu 24.04 LTS"));
        assert_eq!(state.get_entry_items()[0].full_path, "Windows Boot Manager");
    }
}
