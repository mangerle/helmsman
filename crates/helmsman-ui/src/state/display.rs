use super::AppState;
use crate::theme::{ColorPalette, FontScale, SystemColorScheme, ThemeMode};
use grub_config_parser::{
    ConfigLine, DisplayConfigError, GfxMode, GrubColor, MenuVisibility, display_keys,
};

impl AppState {
    /// 切换 os-prober 探测
    pub fn set_os_prober_enabled(&mut self, enabled: bool) {
        let mut vis = MenuVisibility::from_config(&self.draft_config);
        vis.os_prober_disabled = !enabled;
        vis.apply_to(&mut self.draft_config);
    }

    /// 检查是否禁用了恢复模式条目
    pub fn is_recovery_disabled(&self) -> bool {
        MenuVisibility::from_config(&self.draft_config).recovery_disabled
    }

    /// 设置是否禁用恢复模式条目（零侵入隐藏恢复模式内核）
    pub fn set_recovery_disabled(&mut self, disabled: bool) {
        let mut vis = MenuVisibility::from_config(&self.draft_config);
        vis.recovery_disabled = disabled;
        vis.apply_to(&mut self.draft_config);
    }

    /// 检查是否禁用了二级子菜单折叠（拉平菜单）
    pub fn is_submenu_disabled(&self) -> bool {
        MenuVisibility::from_config(&self.draft_config).submenu_disabled
    }

    /// 设置是否禁用二级子菜单（零侵入控制菜单折叠与平铺）
    pub fn set_submenu_disabled(&mut self, disabled: bool) {
        let mut vis = MenuVisibility::from_config(&self.draft_config);
        vis.submenu_disabled = disabled;
        vis.apply_to(&mut self.draft_config);
    }

    /// 读取完整类级可见性策略
    pub fn get_menu_visibility(&self) -> MenuVisibility {
        MenuVisibility::from_config(&self.draft_config)
    }

    /// 写入完整类级可见性策略（类级隐藏；单项隐藏不在本接口范围）
    pub fn set_menu_visibility(&mut self, visibility: MenuVisibility) {
        visibility.apply_to(&mut self.draft_config);
    }

    /// 设置菜单分辨率（GRUB_GFXMODE）；空串或 auto 跟随固件
    ///
    /// # Errors
    /// 当模式格式非法时返回 [`DisplayConfigError`]。
    pub fn set_gfxmode(&mut self, mode: &str) -> Result<(), DisplayConfigError> {
        let trimmed = mode.trim();
        if trimmed.is_empty() {
            return display_keys::set_gfxmode(&mut self.draft_config, None);
        }
        let parsed = GfxMode::parse(trimmed)?;
        display_keys::set_gfxmode(&mut self.draft_config, Some(&parsed))
    }

    /// 获取菜单分辨率原始值
    pub fn get_gfxmode(&self) -> Option<&str> {
        self.draft_config.get("GRUB_GFXMODE")
    }

    /// 设置内核帧缓冲保持策略（GRUB_GFXPAYLOAD）
    ///
    /// # Errors
    /// 当模式格式非法时返回 [`DisplayConfigError`]。
    pub fn set_gfxpayload(&mut self, mode: &str) -> Result<(), DisplayConfigError> {
        let trimmed = mode.trim();
        if trimmed.is_empty() {
            return display_keys::set_gfxpayload(&mut self.draft_config, None);
        }
        let parsed = GfxMode::parse(trimmed)?;
        display_keys::set_gfxpayload(&mut self.draft_config, Some(&parsed))
    }

    /// 获取内核帧缓冲保持策略原始值
    pub fn get_gfxpayload(&self) -> Option<&str> {
        self.draft_config.get("GRUB_GFXPAYLOAD")
    }

    /// 列出常见安全分辨率（探测失败时的回退候选）
    pub fn common_gfx_modes() -> &'static [&'static str] {
        GfxMode::COMMON_MODES
    }

    /// 设置界面视觉主题模式
    pub fn set_theme_mode(&mut self, mode: ThemeMode) {
        self.theme_mode = mode;
    }

    /// 获取当前界面视觉主题模式
    pub fn get_theme_mode(&self) -> ThemeMode {
        self.theme_mode
    }

    /// 设置界面字体缩放级别
    pub fn set_font_scale(&mut self, scale: FontScale) {
        self.font_scale = scale;
    }

    /// 获取当前界面字体缩放级别
    pub fn get_font_scale(&self) -> FontScale {
        self.font_scale
    }

    /// 获取当前生效的语义化调色板
    pub fn current_palette(&self, system_preference: SystemColorScheme) -> ColorPalette {
        let resolved = self.theme_mode.resolve(system_preference);
        ColorPalette::for_theme(resolved)
    }

    /// 设置 GRUB 开机主题描述文件绝对路径
    pub fn set_grub_theme_path(&mut self, path: Option<&str>) {
        match path {
            Some(p) if !p.trim().is_empty() => self.draft_config.set("GRUB_THEME", p.trim()),
            _ => {
                self.draft_config.remove("GRUB_THEME");
            }
        }
    }

    /// 应用解压安装后的主题目录（自动拼接 theme.txt 路径）
    pub fn apply_installed_theme(&mut self, theme_dir: &std::path::Path) {
        let theme_txt = theme_dir.join("theme.txt");
        self.set_grub_theme_path(Some(&theme_txt.to_string_lossy()));
    }

    /// 获取当前配置的 GRUB 开机主题路径
    pub fn get_grub_theme_path(&self) -> Option<&str> {
        self.draft_config.get("GRUB_THEME")
    }

    /// 设置 GRUB 开机背景壁纸路径
    ///
    /// # Errors
    /// 当路径含危险字符时返回 [`DisplayConfigError`]。
    pub fn set_grub_background_path(
        &mut self,
        path: Option<&str>,
    ) -> Result<(), DisplayConfigError> {
        display_keys::set_background(&mut self.draft_config, path)
    }

    /// 获取当前配置的 GRUB 开机背景壁纸路径
    pub fn get_grub_background_path(&self) -> Option<&str> {
        self.draft_config.get("GRUB_BACKGROUND")
    }

    /// 设置 GRUB 终端控制台文本前景色与背景色
    ///
    /// # Errors
    /// 当颜色格式非法时返回 [`DisplayConfigError`]。
    pub fn set_grub_colors(
        &mut self,
        normal: Option<&str>,
        highlight: Option<&str>,
    ) -> Result<(), DisplayConfigError> {
        let normal_color = match normal.map(str::trim) {
            Some(n) if !n.is_empty() => Some(GrubColor::parse(n)?),
            _ => None,
        };
        let highlight_color = match highlight.map(str::trim) {
            Some(h) if !h.is_empty() => Some(GrubColor::parse(h)?),
            _ => None,
        };
        display_keys::set_colors(
            &mut self.draft_config,
            normal_color.as_ref(),
            highlight_color.as_ref(),
        )
    }

    /// 获取 GRUB 终端常规颜色配置
    pub fn get_grub_color_normal(&self) -> Option<&str> {
        self.draft_config.get("GRUB_COLOR_NORMAL")
    }

    /// 获取 GRUB 终端高亮选中颜色配置
    pub fn get_grub_color_highlight(&self) -> Option<&str> {
        self.draft_config.get("GRUB_COLOR_HIGHLIGHT")
    }

    /// 列出当前草稿配置中的所有 `/etc/default/grub` 环境变量键值对
    pub fn list_raw_config_vars(&self) -> Vec<(String, String)> {
        self.draft_config
            .lines
            .iter()
            .filter_map(|line| match line {
                ConfigLine::Assignment { key, value, .. } => Some((key.clone(), value.clone())),
                _ => None,
            })
            .collect()
    }

    /// 安全设置或新增 `/etc/default/grub` 原始环境变量（带变量名与防注入校验）
    pub fn set_raw_config_var(&mut self, key: &str, value: &str) -> Result<(), String> {
        let k = key.trim();
        if k.is_empty() || k.len() > 64 {
            return Err("变量名长度必须为 1-64 个字符".to_string());
        }
        let mut chars = k.chars();
        if let Some(first) = chars.next()
            && !first.is_ascii_alphabetic()
            && first != '_'
        {
            return Err("变量名必须以字母或下划线开头".to_string());
        }
        if !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err("变量名仅允许包含字母、数字与下划线".to_string());
        }
        if value
            .chars()
            .any(|c| c.is_control() || matches!(c, '`' | '$'))
        {
            return Err("变量值禁止包含控制字符或反引号/$ 等潜在命令替换字符".to_string());
        }
        self.draft_config.set(k, value.trim());
        Ok(())
    }

    /// 移除指定 `/etc/default/grub` 环境变量
    pub fn remove_raw_config_var(&mut self, key: &str) -> bool {
        self.draft_config.remove(key.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raw_config_vars_crud_and_validation() {
        let mut state = AppState::new_from_content("GRUB_TIMEOUT=5\n", "");
        assert_eq!(state.list_raw_config_vars().len(), 1);

        assert!(state.set_raw_config_var("123BAD", "val").is_err());
        assert!(state.set_raw_config_var("GRUB_TEST", "$(id)").is_err());
        assert!(
            state
                .set_raw_config_var("GRUB_INIT_TUNE", "480 440 1")
                .is_ok()
        );
        assert_eq!(state.draft_config.get("GRUB_INIT_TUNE"), Some("480 440 1"));
        assert!(state.remove_raw_config_var("GRUB_INIT_TUNE"));
        assert_eq!(state.draft_config.get("GRUB_INIT_TUNE"), None);
    }
}
