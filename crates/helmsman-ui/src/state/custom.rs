use super::{AppState, any_entry_matches};
use grub_boot_reader::CustomBootEntry;

impl AppState {
    /// 添加自定义引导项草稿
    pub fn add_custom_entry(&mut self, entry: CustomBootEntry) {
        self.draft_custom_entries.push(entry);
    }

    /// 移除自定义引导项草稿
    ///
    /// # 语义说明
    /// 仅作用于 Helmsman 受管自定义项草稿；系统内核/Memtest 等发行版生成条目
    /// **不支持删除**，如需不显示请使用类级隐藏（[`grub_config_parser::MenuVisibility`]）。
    pub fn remove_custom_entry(&mut self, id: &str) -> bool {
        let initial_len = self.draft_custom_entries.len();
        self.draft_custom_entries.retain(|e| e.id != id);
        self.draft_custom_entries.len() < initial_len
    }

    /// 判断给定条目路径/标题是否属于系统生成条目（不可删除，仅可隐藏）
    pub fn is_system_entry(&self, full_path: &str) -> bool {
        let is_custom = self
            .draft_custom_entries
            .iter()
            .any(|e| e.id == full_path || e.title == full_path);
        if is_custom {
            return false;
        }
        any_entry_matches(&self.menu_nodes, full_path)
    }

    /// 更新自定义引导项草稿
    pub fn update_custom_entry(&mut self, entry: CustomBootEntry) -> bool {
        for item in &mut self.draft_custom_entries {
            if item.id == entry.id {
                *item = entry;
                return true;
            }
        }
        false
    }

    /// 将自定义项移入指定子菜单（空标题表示移回根层级）
    pub fn move_custom_entry_to_submenu(&mut self, id: &str, submenu_title: &str) -> bool {
        for item in &mut self.draft_custom_entries {
            if item.id == id {
                item.submenu_title = submenu_title.trim().to_string();
                return true;
            }
        }
        false
    }

    /// 将自定义项移回菜单根层级
    pub fn move_custom_entry_to_root(&mut self, id: &str) -> bool {
        self.move_custom_entry_to_submenu(id, "")
    }

    /// 将自定义引导项在同列表内上移一位
    pub fn move_custom_entry_up(&mut self, id: &str) -> bool {
        if let Some(pos) = self.draft_custom_entries.iter().position(|e| e.id == id)
            && pos > 0
        {
            self.draft_custom_entries.swap(pos - 1, pos);
            return true;
        }
        false
    }

    /// 将自定义引导项在同列表内下移一位
    pub fn move_custom_entry_down(&mut self, id: &str) -> bool {
        if let Some(pos) = self.draft_custom_entries.iter().position(|e| e.id == id)
            && pos + 1 < self.draft_custom_entries.len()
        {
            self.draft_custom_entries.swap(pos, pos + 1);
            return true;
        }
        false
    }

    /// 列出当前草稿中出现的子菜单标题（按首次出现顺序，去重）
    pub fn list_custom_submenu_titles(&self) -> Vec<String> {
        let mut titles = Vec::with_capacity(4);
        for item in &self.draft_custom_entries {
            if !item.submenu_title.is_empty() && !titles.contains(&item.submenu_title) {
                titles.push(item.submenu_title.clone());
            }
        }
        titles
    }

    /// 设置条目友好别名
    pub fn set_entry_alias(&mut self, id_or_title: &str, alias: &str) {
        let trimmed = alias.trim();
        if trimmed.is_empty() {
            self.aliases.remove(id_or_title);
        } else {
            self.aliases
                .insert(id_or_title.to_string(), trimmed.to_string());
        }
    }

    /// 获取条目友好别名
    pub fn get_entry_alias(&self, id_or_title: &str) -> Option<&str> {
        self.aliases.get(id_or_title).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_move_custom_entry_up_and_down() {
        let mut state = AppState::new_from_content("", "");
        state.add_custom_entry(CustomBootEntry::new_iso_boot(
            "iso_1",
            "ISO 1",
            "/a.iso",
            "0000-0001",
            "",
        ));
        state.add_custom_entry(CustomBootEntry::new_iso_boot(
            "iso_2",
            "ISO 2",
            "/b.iso",
            "0000-0002",
            "",
        ));

        assert!(!state.move_custom_entry_up("iso_1"));
        assert!(state.move_custom_entry_down("iso_1"));
        assert_eq!(state.draft_custom_entries[0].id, "iso_2");
        assert_eq!(state.draft_custom_entries[1].id, "iso_1");

        assert!(state.move_custom_entry_up("iso_1"));
        assert_eq!(state.draft_custom_entries[0].id, "iso_1");
        assert!(!state.move_custom_entry_down("iso_2"));
    }
}
