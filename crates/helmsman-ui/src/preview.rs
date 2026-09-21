use crate::state::AppState;

/// 模拟开机菜单单项
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewEntry {
    /// 菜单显示索引 (从 0 开始)
    pub index: usize,
    /// 显示标题
    pub title: String,
    /// 当前是否处于高亮选定状态
    pub is_selected: bool,
}

/// 开机引导菜单逻辑效果与倒计时模拟预览模型
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuPreviewModel {
    /// 菜单项列表
    pub entries: Vec<PreviewEntry>,
    /// 当前高亮选中的索引
    pub selected_index: usize,
    /// 配置的初始倒计时秒数
    pub timeout_seconds: i32,
    /// 当前剩余倒计时秒数
    pub remaining_seconds: i32,
    /// 倒计时是否处于运行状态
    pub is_countdown_active: bool,
    /// 是否为静默隐藏菜单模式 (GRUB_TIMEOUT_STYLE=hidden)
    pub is_silent_mode: bool,
}

impl MenuPreviewModel {
    /// 从当前应用状态机构建开机预览模型
    ///
    /// # 设计原理
    /// - **实现初衷**：在图形界面中向用户展示“以此配置重启电脑后开机画面长什么样”。
    /// - **核心优势**：纯内存状态映射，完整反映默认项高亮、超时倒计时与静默模式。
    pub fn from_app_state(state: &AppState) -> Self {
        let entry_items = state.get_entry_items();
        let timeout_seconds = state.get_timeout();
        let timeout_style = state.draft_config.get("GRUB_TIMEOUT_STYLE").unwrap_or("");
        let is_silent_mode = timeout_style == "hidden" && timeout_seconds > 0;

        let mut entries = Vec::with_capacity(entry_items.len());
        let mut selected_index = 0;

        for (idx, item) in entry_items.into_iter().enumerate() {
            if item.is_default {
                selected_index = idx;
            }
            entries.push(PreviewEntry {
                index: idx,
                title: item.title,
                is_selected: false,
            });
        }

        if let Some(entry) = entries.get_mut(selected_index) {
            entry.is_selected = true;
        }

        Self {
            entries,
            selected_index,
            timeout_seconds,
            remaining_seconds: timeout_seconds,
            is_countdown_active: timeout_seconds > 0,
            is_silent_mode,
        }
    }

    /// 模拟倒计时前进 1 秒
    ///
    /// 若倒计时到达 0 则返回 `true`，表示模拟开机完成并自动进入当前选定项。
    pub fn tick_second(&mut self) -> bool {
        if !self.is_countdown_active || self.remaining_seconds <= 0 {
            return false;
        }

        self.remaining_seconds -= 1;
        if self.remaining_seconds == 0 {
            self.is_countdown_active = false;
            true
        } else {
            false
        }
    }

    /// 模拟用户按键盘向下键选择下一个条目（按键将自动中止倒计时）
    pub fn select_next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.is_countdown_active = false;
        if let Some(cur) = self.entries.get_mut(self.selected_index) {
            cur.is_selected = false;
        }
        self.selected_index = (self.selected_index + 1) % self.entries.len();
        if let Some(next) = self.entries.get_mut(self.selected_index) {
            next.is_selected = true;
        }
    }

    /// 模拟用户按键盘向上键选择上一个条目（按键将自动中止倒计时）
    pub fn select_prev(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.is_countdown_active = false;
        if let Some(cur) = self.entries.get_mut(self.selected_index) {
            cur.is_selected = false;
        }
        if self.selected_index == 0 {
            self.selected_index = self.entries.len().saturating_sub(1);
        } else {
            self.selected_index -= 1;
        }
        if let Some(prev) = self.entries.get_mut(self.selected_index) {
            prev.is_selected = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_preview_lifecycle_and_tick() {
        let default_grub = "GRUB_DEFAULT=entry2\nGRUB_TIMEOUT=3\n";
        let grub_cfg =
            "menuentry 'Ubuntu' --id 'entry1' {}\nmenuentry 'Windows' --id 'entry2' {}\n";

        let state = AppState::new_from_content(default_grub, grub_cfg);
        let mut preview = MenuPreviewModel::from_app_state(&state);

        assert_eq!(preview.entries.len(), 2);
        assert_eq!(preview.selected_index, 1);
        assert!(preview.entries[1].is_selected);
        assert_eq!(preview.remaining_seconds, 3);
        assert!(preview.is_countdown_active);

        // 步进 1 秒
        assert!(!preview.tick_second());
        assert_eq!(preview.remaining_seconds, 2);

        // 步进 2 秒
        assert!(!preview.tick_second());
        assert_eq!(preview.remaining_seconds, 1);

        // 步进到 0 秒，触发启动
        assert!(preview.tick_second());
        assert_eq!(preview.remaining_seconds, 0);
        assert!(!preview.is_countdown_active);
    }

    #[test]
    fn test_menu_preview_key_navigation_stops_countdown() {
        let default_grub = "GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n";
        let grub_cfg = "menuentry 'Ubuntu' {}\nmenuentry 'Windows' {}\n";

        let state = AppState::new_from_content(default_grub, grub_cfg);
        let mut preview = MenuPreviewModel::from_app_state(&state);

        assert_eq!(preview.selected_index, 0);
        assert!(preview.is_countdown_active);

        // 用户按向下键选择下一项
        preview.select_next();
        assert_eq!(preview.selected_index, 1);
        assert!(!preview.is_countdown_active, "按键应中止自动倒计时");

        // 用户按向上键切回上一项
        preview.select_prev();
        assert_eq!(preview.selected_index, 0);
    }
}
