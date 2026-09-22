use std::env;
use std::fmt;

/// 支持的界面语言
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// 简体中文 (默认首选)
    #[default]
    ZhCn,
    /// 英文 (备选)
    EnUs,
}

impl Language {
    /// 根据系统 Locale 字符串（如 "zh_CN.UTF-8"、"en_US"）解析
    pub fn from_locale(locale: &str) -> Self {
        let lower = locale.to_ascii_lowercase();
        if lower.starts_with("zh") {
            Language::ZhCn
        } else if lower.starts_with("en") {
            Language::EnUs
        } else {
            // 未知语言默认英文，避免非中文环境被强制中文
            Language::EnUs
        }
    }

    /// 获取语言标签代码
    pub fn code(&self) -> &'static str {
        match self {
            Language::ZhCn => "zh-CN",
            Language::EnUs => "en-US",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Language::ZhCn => write!(f, "简体中文"),
            Language::EnUs => write!(f, "English"),
        }
    }
}

/// 国际化文本键枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextKey {
    /// 应用标题
    AppTitle,
    /// 保存并应用按钮
    SaveButton,
    /// 放弃并重置按钮
    ResetButton,
    /// 差异预览标题
    DiffPreviewTitle,
    /// 高危警告标题
    RiskWarningTitle,
    /// 默认启动项标签
    DefaultEntryLabel,
    /// 倒计时标签
    TimeoutLabel,
    /// 内核参数标签
    KernelParamsLabel,
    /// 确认提交标题
    ConfirmApplyTitle,
    /// 确认提交说明
    ConfirmApplyMessage,
    /// 无更改提示
    NoChangesMessage,
    /// 快照创建通知
    SnapshotCreatedNotice,
    /// 回滚成功通知
    RollbackSuccessNotice,
    // ---- 影响分析 (impact) ----
    /// 影响：倒计时标题
    ImpactTimeoutTitle,
    /// 影响：倒计时改为 0
    ImpactTimeoutToZero,
    /// 影响：倒计时调整
    ImpactTimeoutAdjusted,
    /// 影响：默认项标题
    ImpactDefaultTitle,
    /// 影响：默认项 saved
    ImpactDefaultSaved,
    /// 影响：默认项切换
    ImpactDefaultSwitched,
    /// 影响：启用 os-prober 标题
    ImpactOsProberEnableTitle,
    /// 影响：启用 os-prober 说明
    ImpactOsProberEnableExplain,
    /// 影响：禁用 os-prober 标题
    ImpactOsProberDisableTitle,
    /// 影响：禁用 os-prober 说明
    ImpactOsProberDisableExplain,
    /// 影响：启用主题标题
    ImpactThemeEnableTitle,
    /// 影响：启用主题说明
    ImpactThemeEnableExplain,
    /// 影响：禁用主题标题
    ImpactThemeDisableTitle,
    /// 影响：禁用主题说明
    ImpactThemeDisableExplain,
    /// 影响：切换主题标题
    ImpactThemeSwitchTitle,
    /// 影响：切换主题说明
    ImpactThemeSwitchExplain,
    /// 影响：设置壁纸标题
    ImpactBgSetTitle,
    /// 影响：设置壁纸说明
    ImpactBgSetExplain,
    /// 影响：清除壁纸标题
    ImpactBgClearTitle,
    /// 影响：清除壁纸说明
    ImpactBgClearExplain,
    /// 影响：更换壁纸标题
    ImpactBgChangeTitle,
    /// 影响：更换壁纸说明
    ImpactBgChangeExplain,
    /// 影响：nomodeset 标题
    ImpactNomodesetTitle,
    /// 影响：nomodeset 说明
    ImpactNomodesetExplain,
    /// 影响：移除 quiet 标题
    ImpactQuietOffTitle,
    /// 影响：移除 quiet 说明
    ImpactQuietOffExplain,
    /// 影响：移除 splash 标题
    ImpactSplashOffTitle,
    /// 影响：移除 splash 说明
    ImpactSplashOffExplain,
    // ---- 安全校验 (validator) ----
    /// 风险等级：安全
    RiskSafe,
    /// 风险等级：警告
    RiskWarning,
    /// 风险等级：致命
    RiskDanger,
    /// 校验：静默零倒计时标题
    ValidTimeoutZeroTitle,
    /// 校验：静默零倒计时说明
    ValidTimeoutZeroMessage,
    /// 校验：默认项为空标题
    ValidEmptyDefaultTitle,
    /// 校验：默认项为空说明
    ValidEmptyDefaultMessage,
    /// 校验：内核参数为空标题
    ValidEmptyCmdlineTitle,
    /// 校验：内核参数为空说明
    ValidEmptyCmdlineMessage,
    /// 校验：主题路径异常标题
    ValidThemePathTitle,
    /// 校验：主题路径异常说明
    ValidThemePathMessage,
    /// 校验：壁纸格式标题
    ValidBgFormatTitle,
    /// 校验：壁纸格式说明
    ValidBgFormatMessage,
}

/// 获取指定语言与键对应的本地化文本
pub fn get_text(lang: Language, key: TextKey) -> &'static str {
    match lang {
        Language::ZhCn => match key {
            TextKey::AppTitle => "Helmsman - GRUB 引导管理",
            TextKey::SaveButton => "应用更改",
            TextKey::ResetButton => "放弃修改",
            TextKey::DiffPreviewTitle => "配置变更差异预览",
            TextKey::RiskWarningTitle => "高危配置安全警告",
            TextKey::DefaultEntryLabel => "默认启动项",
            TextKey::TimeoutLabel => "等待倒计时 (秒)",
            TextKey::KernelParamsLabel => "全局内核引导参数",
            TextKey::ConfirmApplyTitle => "确认提交引导配置",
            TextKey::ConfirmApplyMessage => "即将把新配置写入系统并更新引导脚本，是否继续？",
            TextKey::NoChangesMessage => "配置无变化，无需保存。",
            TextKey::SnapshotCreatedNotice => "已成功创建灾难恢复快照",
            TextKey::RollbackSuccessNotice => "已成功恢复至指定历史快照",
            TextKey::ImpactTimeoutTitle => "开机倒计时调整",
            TextKey::ImpactTimeoutToZero => {
                "开机等待时间由 {orig} 秒调整为 0 秒，开机时将不显示选择菜单，直接跳入默认启动项。"
            }
            TextKey::ImpactTimeoutAdjusted => {
                "开机等待倒计时由 {orig} 秒调整为 {draft} 秒，用户将有更充裕或更精简的时间选择启动项。"
            }
            TextKey::ImpactDefaultTitle => "默认启动项切换",
            TextKey::ImpactDefaultSaved => {
                "默认启动项切换为记忆模式 (saved)，系统将自动记录并默认启动上一次成功进入的系统。"
            }
            TextKey::ImpactDefaultSwitched => {
                "默认启动项由 '{orig}' 切换为 '{draft}'，开机倒计时结束后将默认启动该项。"
            }
            TextKey::ImpactOsProberEnableTitle => "启用多系统探测 (os-prober)",
            TextKey::ImpactOsProberEnableExplain => {
                "生成引导时将自动扫描硬盘上的 Windows 或其他 Linux 系统，并在开机菜单中添加对应条目。"
            }
            TextKey::ImpactOsProberDisableTitle => "禁用多系统探测 (os-prober)",
            TextKey::ImpactOsProberDisableExplain => {
                "生成引导时将跳过其他硬盘分区的系统扫描，可加快引导生成速度，但将隐藏其他系统。"
            }
            TextKey::ImpactThemeEnableTitle => "启用开机视觉主题",
            TextKey::ImpactThemeEnableExplain => {
                "开机将应用图形化主题包 '{theme}'，提供背景壁纸、菜单框与倒计时样式美化。"
            }
            TextKey::ImpactThemeDisableTitle => "禁用开机视觉主题",
            TextKey::ImpactThemeDisableExplain => {
                "清除了主题包 '{theme}'，开机时将恢复为简单黑底文本菜单。"
            }
            TextKey::ImpactThemeSwitchTitle => "切换开机视觉主题",
            TextKey::ImpactThemeSwitchExplain => "开机主题由 '{old}' 切换为 '{new}'。",
            TextKey::ImpactBgSetTitle => "设置自定义开机壁纸",
            TextKey::ImpactBgSetExplain => "开机引导背景将显示图片 '{path}'。",
            TextKey::ImpactBgClearTitle => "清除自定义开机壁纸",
            TextKey::ImpactBgClearExplain => "开机将使用默认单色背景。",
            TextKey::ImpactBgChangeTitle => "更换自定义开机壁纸",
            TextKey::ImpactBgChangeExplain => "壁纸由 '{old}' 更换为 '{new}'。",
            TextKey::ImpactNomodesetTitle => "添加通用显示驱动模式 (nomodeset)",
            TextKey::ImpactNomodesetExplain => {
                "内核将在图形驱动加载前使用标准通用驱动，适用于因显卡驱动异常导致的开机黑屏排障。"
            }
            TextKey::ImpactQuietOffTitle => "关闭静默启动 (移除 quiet)",
            TextKey::ImpactQuietOffExplain => {
                "系统开机时将在屏幕上完整滚屏打印内核加载与服务启动日志，便于精确定位启动卡顿点。"
            }
            TextKey::ImpactSplashOffTitle => "关闭开机动画 (移除 splash)",
            TextKey::ImpactSplashOffExplain => {
                "开机时将不再显示图形 Logo 或转圈动画，直接呈现文字终端。"
            }
            TextKey::RiskSafe => "安全",
            TextKey::RiskWarning => "警告",
            TextKey::RiskDanger => "致命危险",
            TextKey::ValidTimeoutZeroTitle => "倒计时为 0 且处于静默模式",
            TextKey::ValidTimeoutZeroMessage => {
                "开机等待时间设为 0 且未显示菜单，将导致系统开机时完全无法通过按 Esc 唤出救援菜单。一旦默认内核异常，系统将无法自愈进入恢复模式！"
            }
            TextKey::ValidEmptyDefaultTitle => "默认启动项为空",
            TextKey::ValidEmptyDefaultMessage => {
                "未指定明确的默认启动项，GRUB 将默认选择菜单第一项。若第一项非正常内核，可能导致启动异常。"
            }
            TextKey::ValidEmptyCmdlineTitle => "内核引导参数为空",
            TextKey::ValidEmptyCmdlineMessage => {
                "全局内核参数完全为空，可能会丢失关键驱动参数或根分区挂载选项。"
            }
            TextKey::ValidThemePathTitle => "主题路径格式异常",
            TextKey::ValidThemePathMessage => {
                "配置的主题路径 '{path}' 未以 'theme.txt' 结尾。GRUB 官方规范要求 GRUB_THEME 必须指向具体的主题描述文件。"
            }
            TextKey::ValidBgFormatTitle => "背景壁纸格式不支持",
            TextKey::ValidBgFormatMessage => {
                "配置的背景图片 '{path}' 扩展名不受支持。GRUB 仅支持 PNG、JPEG 或 TGA 格式的位图。"
            }
        },
        Language::EnUs => match key {
            TextKey::AppTitle => "Helmsman - GRUB Boot Manager",
            TextKey::SaveButton => "Apply Changes",
            TextKey::ResetButton => "Discard Changes",
            TextKey::DiffPreviewTitle => "Configuration Diff Preview",
            TextKey::RiskWarningTitle => "Security Risk Warning",
            TextKey::DefaultEntryLabel => "Default Boot Entry",
            TextKey::TimeoutLabel => "Boot Timeout (Seconds)",
            TextKey::KernelParamsLabel => "Global Kernel Parameters",
            TextKey::ConfirmApplyTitle => "Confirm Configuration Changes",
            TextKey::ConfirmApplyMessage => {
                "New configuration will be written and bootloader updated. Continue?"
            }
            TextKey::NoChangesMessage => "No changes detected.",
            TextKey::SnapshotCreatedNotice => "Snapshot created successfully",
            TextKey::RollbackSuccessNotice => "Rolled back to snapshot successfully",
            TextKey::ImpactTimeoutTitle => "Boot timeout adjusted",
            TextKey::ImpactTimeoutToZero => {
                "Boot wait time changed from {orig}s to 0s; the menu will be hidden and the default entry boots immediately."
            }
            TextKey::ImpactTimeoutAdjusted => {
                "Boot countdown changed from {orig}s to {draft}s, giving more or less time to pick an entry."
            }
            TextKey::ImpactDefaultTitle => "Default boot entry changed",
            TextKey::ImpactDefaultSaved => {
                "Default entry switched to saved mode; GRUB will remember and boot the last successful system."
            }
            TextKey::ImpactDefaultSwitched => {
                "Default entry changed from '{orig}' to '{draft}'; it will boot after the countdown."
            }
            TextKey::ImpactOsProberEnableTitle => "Enable os-prober",
            TextKey::ImpactOsProberEnableExplain => {
                "Boot generation will scan disks for Windows or other Linux systems and add menu entries."
            }
            TextKey::ImpactOsProberDisableTitle => "Disable os-prober",
            TextKey::ImpactOsProberDisableExplain => {
                "Boot generation will skip scanning other partitions, speeding up updates but hiding other systems."
            }
            TextKey::ImpactThemeEnableTitle => "Enable boot theme",
            TextKey::ImpactThemeEnableExplain => {
                "Boot will apply theme package '{theme}' with background, menu box and countdown styling."
            }
            TextKey::ImpactThemeDisableTitle => "Disable boot theme",
            TextKey::ImpactThemeDisableExplain => {
                "Removed theme '{theme}'; boot will fall back to a plain text menu."
            }
            TextKey::ImpactThemeSwitchTitle => "Switch boot theme",
            TextKey::ImpactThemeSwitchExplain => "Boot theme changed from '{old}' to '{new}'.",
            TextKey::ImpactBgSetTitle => "Set custom boot wallpaper",
            TextKey::ImpactBgSetExplain => "Boot background will show image '{path}'.",
            TextKey::ImpactBgClearTitle => "Clear custom boot wallpaper",
            TextKey::ImpactBgClearExplain => "Boot will use the default solid background.",
            TextKey::ImpactBgChangeTitle => "Change custom boot wallpaper",
            TextKey::ImpactBgChangeExplain => "Wallpaper changed from '{old}' to '{new}'.",
            TextKey::ImpactNomodesetTitle => "Add generic graphics mode (nomodeset)",
            TextKey::ImpactNomodesetExplain => {
                "Kernel will use generic display drivers before GPU drivers load; useful for black-screen debugging."
            }
            TextKey::ImpactQuietOffTitle => "Disable silent boot (remove quiet)",
            TextKey::ImpactQuietOffExplain => {
                "Kernel and service logs will scroll on screen during boot for diagnosing stalls."
            }
            TextKey::ImpactSplashOffTitle => "Disable boot splash (remove splash)",
            TextKey::ImpactSplashOffExplain => {
                "Boot will show a text console instead of a graphical logo or spinner."
            }
            TextKey::RiskSafe => "Safe",
            TextKey::RiskWarning => "Warning",
            TextKey::RiskDanger => "Critical",
            TextKey::ValidTimeoutZeroTitle => "Timeout is 0 with silent mode",
            TextKey::ValidTimeoutZeroMessage => {
                "Timeout 0 with no menu means Esc cannot open the rescue menu. If the default kernel fails, recovery is impossible!"
            }
            TextKey::ValidEmptyDefaultTitle => "Default entry is empty",
            TextKey::ValidEmptyDefaultMessage => {
                "No default entry is set; GRUB boots the first menu item. If that is not a normal kernel, boot may fail."
            }
            TextKey::ValidEmptyCmdlineTitle => "Kernel parameters are empty",
            TextKey::ValidEmptyCmdlineMessage => {
                "Global kernel parameters are empty; critical driver or root-mount options may be missing."
            }
            TextKey::ValidThemePathTitle => "Unusual theme path",
            TextKey::ValidThemePathMessage => {
                "Theme path '{path}' does not end with 'theme.txt'. GRUB_THEME must point at the theme descriptor."
            }
            TextKey::ValidBgFormatTitle => "Unsupported wallpaper format",
            TextKey::ValidBgFormatMessage => {
                "Background image '{path}' has an unsupported extension. GRUB accepts PNG, JPEG or TGA only."
            }
        },
    }
}

/// 支持动态参数插值的文本格式化
pub fn format_text(lang: Language, key: TextKey, args: &[(&str, &str)]) -> String {
    let raw = get_text(lang, key);
    let mut result = raw.to_string();
    for (k, v) in args {
        let placeholder = format!("{{{}}}", k);
        result = result.replace(&placeholder, v);
    }
    result
}

/// 探测当前宿主操作系统的默认界面语言
pub fn detect_system_language() -> Language {
    detect_language_from_lookup(|k| env::var(k).ok())
}

/// 内部支持依赖注入测试的环境语言探测实现
pub(crate) fn detect_language_from_lookup<F>(lookup: F) -> Language
where
    F: Fn(&str) -> Option<String>,
{
    for var in &["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(val) = lookup(var)
            && !val.trim().is_empty()
        {
            return Language::from_locale(&val);
        }
    }
    Language::EnUs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locale_parsing() {
        assert_eq!(Language::from_locale("zh_CN.UTF-8"), Language::ZhCn);
        assert_eq!(Language::from_locale("zh_TW"), Language::ZhCn);
        assert_eq!(Language::from_locale("en_US.UTF-8"), Language::EnUs);
        assert_eq!(Language::from_locale("en_GB"), Language::EnUs);
        assert_eq!(Language::from_locale("fr_FR.UTF-8"), Language::EnUs);
    }

    #[test]
    fn test_text_retrieval() {
        assert_eq!(get_text(Language::ZhCn, TextKey::SaveButton), "应用更改");
        assert_eq!(
            get_text(Language::EnUs, TextKey::SaveButton),
            "Apply Changes"
        );
    }

    #[test]
    fn test_format_text_interpolation() {
        // 测试无插值占位符的情况
        let res = format_text(Language::ZhCn, TextKey::AppTitle, &[]);
        assert_eq!(res, "Helmsman - GRUB 引导管理");
    }

    #[test]
    fn test_detect_language_mock() {
        let zh = detect_language_from_lookup(|k| {
            if k == "LANG" {
                Some("zh_CN.UTF-8".to_string())
            } else {
                None
            }
        });
        assert_eq!(zh, Language::ZhCn);

        let en = detect_language_from_lookup(|k| {
            if k == "LC_ALL" {
                Some("en_US.UTF-8".to_string())
            } else {
                None
            }
        });
        assert_eq!(en, Language::EnUs);

        let fallback = detect_language_from_lookup(|_| None);
        assert_eq!(fallback, Language::EnUs);
    }
}
