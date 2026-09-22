use crate::i18n::{Language, TextKey, format_text, get_text};
use grub_config_parser::GrubConfigFile;

/// 单项配置变更语义影响条目
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactItem {
    /// 影响条目标题
    pub title: String,
    /// 通俗解释与效果说明
    pub explanation: String,
}

/// 引导变更影响分析综合报告
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImpactReport {
    /// 影响条目清单
    pub items: Vec<ImpactItem>,
}

impl ImpactReport {
    /// 是否包含任何语义影响
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// 引导配置变更语义影响分析引擎
pub struct ImpactAnalyzer;

impl ImpactAnalyzer {
    /// 对比原始配置与草稿配置，生成指定语言的通俗影响分析报告
    ///
    /// # 设计原理
    /// - **实现初衷**：普通用户难以从 `GRUB_CMDLINE_LINUX_DEFAULT="nomodeset"` 等底层键值理解对启动的实际影响。
    /// - **核心优势**：将底层参数变化转换为清晰的系统行为描述，并按界面语言输出文案。
    pub fn analyze(
        original: &GrubConfigFile,
        draft: &GrubConfigFile,
        lang: Language,
    ) -> ImpactReport {
        let mut items = Vec::new();

        // 1. 分析倒计时变更
        let orig_timeout = original.get("GRUB_TIMEOUT").unwrap_or("5");
        let draft_timeout = draft.get("GRUB_TIMEOUT").unwrap_or("5");
        if orig_timeout != draft_timeout {
            let explanation = if draft_timeout == "0" {
                format_text(
                    lang,
                    TextKey::ImpactTimeoutToZero,
                    &[("orig", orig_timeout)],
                )
            } else {
                format_text(
                    lang,
                    TextKey::ImpactTimeoutAdjusted,
                    &[("orig", orig_timeout), ("draft", draft_timeout)],
                )
            };
            items.push(ImpactItem {
                title: get_text(lang, TextKey::ImpactTimeoutTitle).to_string(),
                explanation,
            });
        }

        // 2. 分析默认启动项变更
        let orig_default = original.get("GRUB_DEFAULT").unwrap_or("0");
        let draft_default = draft.get("GRUB_DEFAULT").unwrap_or("0");
        if orig_default != draft_default {
            let explanation = if draft_default == "saved" {
                get_text(lang, TextKey::ImpactDefaultSaved).to_string()
            } else {
                format_text(
                    lang,
                    TextKey::ImpactDefaultSwitched,
                    &[("orig", orig_default), ("draft", draft_default)],
                )
            };
            items.push(ImpactItem {
                title: get_text(lang, TextKey::ImpactDefaultTitle).to_string(),
                explanation,
            });
        }

        // 3. 分析内核引导参数变更
        let orig_cmdline = original
            .get("GRUB_CMDLINE_LINUX_DEFAULT")
            .unwrap_or("")
            .to_string();
        let draft_cmdline = draft
            .get("GRUB_CMDLINE_LINUX_DEFAULT")
            .unwrap_or("")
            .to_string();

        if orig_cmdline != draft_cmdline {
            Self::analyze_cmdline_diff(&orig_cmdline, &draft_cmdline, lang, &mut items);
        }

        // 4. 分析 os-prober 探测开关变更
        let orig_prober = original.get("GRUB_DISABLE_OS_PROBER");
        let draft_prober = draft.get("GRUB_DISABLE_OS_PROBER");
        if orig_prober != draft_prober {
            if draft_prober.is_none() || draft_prober == Some("false") {
                items.push(ImpactItem {
                    title: get_text(lang, TextKey::ImpactOsProberEnableTitle).to_string(),
                    explanation: get_text(lang, TextKey::ImpactOsProberEnableExplain).to_string(),
                });
            } else {
                items.push(ImpactItem {
                    title: get_text(lang, TextKey::ImpactOsProberDisableTitle).to_string(),
                    explanation: get_text(lang, TextKey::ImpactOsProberDisableExplain).to_string(),
                });
            }
        }

        // 5. 分析开机主题变更
        let orig_theme = original.get("GRUB_THEME");
        let draft_theme = draft.get("GRUB_THEME");
        if orig_theme != draft_theme {
            match (orig_theme, draft_theme) {
                (None, Some(new_t)) => {
                    items.push(ImpactItem {
                        title: get_text(lang, TextKey::ImpactThemeEnableTitle).to_string(),
                        explanation: format_text(
                            lang,
                            TextKey::ImpactThemeEnableExplain,
                            &[("theme", new_t)],
                        ),
                    });
                }
                (Some(old_t), None) => {
                    items.push(ImpactItem {
                        title: get_text(lang, TextKey::ImpactThemeDisableTitle).to_string(),
                        explanation: format_text(
                            lang,
                            TextKey::ImpactThemeDisableExplain,
                            &[("theme", old_t)],
                        ),
                    });
                }
                (Some(old_t), Some(new_t)) => {
                    items.push(ImpactItem {
                        title: get_text(lang, TextKey::ImpactThemeSwitchTitle).to_string(),
                        explanation: format_text(
                            lang,
                            TextKey::ImpactThemeSwitchExplain,
                            &[("old", old_t), ("new", new_t)],
                        ),
                    });
                }
                (None, None) => {}
            }
        }

        // 6. 分析背景壁纸变更
        let orig_bg = original.get("GRUB_BACKGROUND");
        let draft_bg = draft.get("GRUB_BACKGROUND");
        if orig_bg != draft_bg {
            match (orig_bg, draft_bg) {
                (None, Some(new_bg)) => {
                    items.push(ImpactItem {
                        title: get_text(lang, TextKey::ImpactBgSetTitle).to_string(),
                        explanation: format_text(
                            lang,
                            TextKey::ImpactBgSetExplain,
                            &[("path", new_bg)],
                        ),
                    });
                }
                (Some(_), None) => {
                    items.push(ImpactItem {
                        title: get_text(lang, TextKey::ImpactBgClearTitle).to_string(),
                        explanation: get_text(lang, TextKey::ImpactBgClearExplain).to_string(),
                    });
                }
                (Some(old_bg), Some(new_bg)) => {
                    items.push(ImpactItem {
                        title: get_text(lang, TextKey::ImpactBgChangeTitle).to_string(),
                        explanation: format_text(
                            lang,
                            TextKey::ImpactBgChangeExplain,
                            &[("old", old_bg), ("new", new_bg)],
                        ),
                    });
                }
                (None, None) => {}
            }
        }

        ImpactReport { items }
    }

    /// 分析内核命令行参数的具体增删语义
    fn analyze_cmdline_diff(orig: &str, draft: &str, lang: Language, items: &mut Vec<ImpactItem>) {
        let orig_tokens: Vec<&str> = orig.split_whitespace().collect();
        let draft_tokens: Vec<&str> = draft.split_whitespace().collect();

        if !orig_tokens.contains(&"nomodeset") && draft_tokens.contains(&"nomodeset") {
            items.push(ImpactItem {
                title: get_text(lang, TextKey::ImpactNomodesetTitle).to_string(),
                explanation: get_text(lang, TextKey::ImpactNomodesetExplain).to_string(),
            });
        }

        if orig_tokens.contains(&"quiet") && !draft_tokens.contains(&"quiet") {
            items.push(ImpactItem {
                title: get_text(lang, TextKey::ImpactQuietOffTitle).to_string(),
                explanation: get_text(lang, TextKey::ImpactQuietOffExplain).to_string(),
            });
        }

        if orig_tokens.contains(&"splash") && !draft_tokens.contains(&"splash") {
            items.push(ImpactItem {
                title: get_text(lang, TextKey::ImpactSplashOffTitle).to_string(),
                explanation: get_text(lang, TextKey::ImpactSplashOffExplain).to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grub_config_parser::parse_grub_config;

    #[test]
    fn test_impact_timeout_and_default_entry() {
        let orig = parse_grub_config("GRUB_DEFAULT=0\nGRUB_TIMEOUT=5\n");
        let draft = parse_grub_config("GRUB_DEFAULT=saved\nGRUB_TIMEOUT=10\n");

        let report = ImpactAnalyzer::analyze(&orig, &draft, Language::ZhCn);
        assert_eq!(report.items.len(), 2);
        assert_eq!(report.items[0].title, "开机倒计时调整");
        assert!(report.items[0].explanation.contains("5 秒调整为 10 秒"));
        assert_eq!(report.items[1].title, "默认启动项切换");
        assert!(report.items[1].explanation.contains("记忆模式 (saved)"));

        let en = ImpactAnalyzer::analyze(&orig, &draft, Language::EnUs);
        assert_eq!(en.items[0].title, "Boot timeout adjusted");
    }

    #[test]
    fn test_impact_kernel_params_nomodeset() {
        let orig = parse_grub_config("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\"\n");
        let draft = parse_grub_config("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash nomodeset\"\n");

        let report = ImpactAnalyzer::analyze(&orig, &draft, Language::ZhCn);
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].title, "添加通用显示驱动模式 (nomodeset)");
        assert!(report.items[0].explanation.contains("显卡驱动异常"));
    }

    #[test]
    fn test_impact_remove_quiet_and_splash() {
        let orig = parse_grub_config("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\"\n");
        let draft = parse_grub_config("GRUB_CMDLINE_LINUX_DEFAULT=\"\"\n");

        let report = ImpactAnalyzer::analyze(&orig, &draft, Language::ZhCn);
        assert_eq!(report.items.len(), 2);
        assert!(report.items.iter().any(|i| i.title.contains("quiet")));
        assert!(report.items.iter().any(|i| i.title.contains("splash")));
    }
}
