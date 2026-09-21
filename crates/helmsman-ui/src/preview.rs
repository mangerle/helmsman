use crate::state::AppState;
use grub_boot_reader::GrubThemeDefinition;
use std::path::PathBuf;

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

/// 界面绝对像素矩形区域
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// 开机界面高保真视觉渲染布局快照
#[derive(Debug, Clone, PartialEq)]
pub struct VisualLayoutSnapshot {
    /// 模拟的屏幕宽度
    pub screen_width: u32,
    /// 模拟的屏幕高度
    pub screen_height: u32,
    /// 桌面背景壁纸路径
    pub background_image: Option<PathBuf>,
    /// 桌面背景纯色值 (如 "#1e1e2e" 或 "#000000")
    pub background_color: String,
    /// 顶部/全局标题文字
    pub title_text: Option<String>,
    /// 引导菜单框绝对几何区域
    pub menu_rect: Rect,
    /// 普通条目文字颜色
    pub item_color: String,
    /// 当前选中条目文字颜色
    pub selected_item_color: String,
    /// 倒计时进度条绝对几何区域 (若有)
    pub progress_bar_rect: Option<Rect>,
    /// 倒计时进度条前景色
    pub progress_bar_fg_color: Option<String>,
    /// 倒计时进度条背景色
    pub progress_bar_bg_color: Option<String>,
    /// 倒计时剩余进度比例 (0.0 到 1.0)
    pub progress_ratio: f32,
}

/// 开机引导菜单逻辑效果与倒计时模拟预览模型
#[derive(Debug, Clone, PartialEq)]
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
    /// 关联的 GRUB 主题完整定义 (可选)
    pub theme: Option<GrubThemeDefinition>,
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
            theme: None,
        }
    }

    /// 链式挂载 GRUB 主题定义
    pub fn with_theme_definition(mut self, theme: GrubThemeDefinition) -> Self {
        self.theme = Some(theme);
        self
    }

    /// 计算在指定屏幕分辨率下的高保真视觉排版布局
    pub fn compute_visual_layout(
        &self,
        screen_width: u32,
        screen_height: u32,
    ) -> VisualLayoutSnapshot {
        let progress_ratio = if self.timeout_seconds > 0 {
            (self.remaining_seconds as f32 / self.timeout_seconds as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        if let Some(ref t) = self.theme {
            Self::compute_themed_layout(t, screen_width, screen_height, progress_ratio)
        } else {
            Self::compute_default_layout(screen_width, screen_height, progress_ratio)
        }
    }

    /// 计算基于 theme.txt 的主题布局
    fn compute_themed_layout(
        theme: &GrubThemeDefinition,
        screen_width: u32,
        screen_height: u32,
        progress_ratio: f32,
    ) -> VisualLayoutSnapshot {
        let (menu_rect, item_color, selected_item_color) = if let Some(m) = theme.boot_menu() {
            let x = m
                .left
                .map(|d| d.resolve_pixels(screen_width))
                .unwrap_or((screen_width / 4) as i32);
            let y = m
                .top
                .map(|d| d.resolve_pixels(screen_height))
                .unwrap_or((screen_height / 4) as i32);
            let width = m
                .width
                .map(|d| d.resolve_pixels(screen_width).max(0) as u32)
                .unwrap_or(screen_width / 2);
            let height = m
                .height
                .map(|d| d.resolve_pixels(screen_height).max(0) as u32)
                .unwrap_or(screen_height / 2);

            let ic = m
                .item_color
                .clone()
                .unwrap_or_else(|| "#c0c0c0".to_string());
            let sc = m
                .selected_item_color
                .clone()
                .unwrap_or_else(|| "#ffffff".to_string());
            (
                Rect {
                    x,
                    y,
                    width,
                    height,
                },
                ic,
                sc,
            )
        } else {
            (
                Rect {
                    x: (screen_width / 4) as i32,
                    y: (screen_height / 4) as i32,
                    width: screen_width / 2,
                    height: screen_height / 2,
                },
                "#c0c0c0".to_string(),
                "#ffffff".to_string(),
            )
        };

        let (progress_bar_rect, pb_fg, pb_bg) = if let Some(p) = theme.progress_bar() {
            let x = p
                .left
                .map(|d| d.resolve_pixels(screen_width))
                .unwrap_or((screen_width / 4) as i32);
            let y = p
                .top
                .map(|d| d.resolve_pixels(screen_height))
                .unwrap_or((screen_height * 3 / 4) as i32);
            let width = p
                .width
                .map(|d| d.resolve_pixels(screen_width).max(0) as u32)
                .unwrap_or(screen_width / 2);
            let height = p
                .height
                .map(|d| d.resolve_pixels(screen_height).max(0) as u32)
                .unwrap_or(20);

            (
                Some(Rect {
                    x,
                    y,
                    width,
                    height,
                }),
                p.fg_color.clone(),
                p.bg_color.clone(),
            )
        } else {
            (None, None, None)
        };

        VisualLayoutSnapshot {
            screen_width,
            screen_height,
            background_image: theme.desktop_image.clone(),
            background_color: theme
                .desktop_color
                .clone()
                .unwrap_or_else(|| "#000000".to_string()),
            title_text: theme.title_text.clone(),
            menu_rect,
            item_color,
            selected_item_color,
            progress_bar_rect,
            progress_bar_fg_color: pb_fg,
            progress_bar_bg_color: pb_bg,
            progress_ratio,
        }
    }

    /// 计算经典黑底纯文本 GRUB 布局
    fn compute_default_layout(
        screen_width: u32,
        screen_height: u32,
        progress_ratio: f32,
    ) -> VisualLayoutSnapshot {
        let menu_rect = Rect {
            x: (screen_width / 5) as i32,
            y: (screen_height / 5) as i32,
            width: screen_width * 3 / 5,
            height: screen_height * 3 / 5,
        };

        VisualLayoutSnapshot {
            screen_width,
            screen_height,
            background_image: None,
            background_color: "#000000".to_string(),
            title_text: Some("GNU GRUB".to_string()),
            menu_rect,
            item_color: "#c0c0c0".to_string(),
            selected_item_color: "#ffffff".to_string(),
            progress_bar_rect: None,
            progress_bar_fg_color: None,
            progress_bar_bg_color: None,
            progress_ratio,
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
