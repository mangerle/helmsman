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
            Language::ZhCn
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
}

/// 获取指定语言与键对应的本地化文本
pub fn get_text(lang: Language, key: TextKey) -> &'static str {
    match lang {
        Language::ZhCn => match key {
            TextKey::AppTitle => "Helmsman (舵手) - GRUB 引导管理",
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
    Language::ZhCn
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
        assert_eq!(Language::from_locale("fr_FR.UTF-8"), Language::ZhCn);
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
        assert_eq!(res, "Helmsman (舵手) - GRUB 引导管理");
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
        assert_eq!(fallback, Language::ZhCn);
    }
}
