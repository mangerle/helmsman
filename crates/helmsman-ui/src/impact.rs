use grub_config_parser::GrubConfigFile;

/// 单项配置变更语义影响条目
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImpactItem {
    /// 影响条目标题
    pub title: String,
    /// 通俗中文解释与效果说明
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
    /// 对比原始配置与草稿配置，生成通俗易懂的中文影响分析报告
    ///
    /// # 设计原理
    /// - **实现初衷**：普通用户难以从 `GRUB_CMDLINE_LINUX_DEFAULT="nomodeset"` 等底层键值理解对启动的实际影响。
    /// - **核心优势**：将底层参数变化转换为清晰的系统行为描述，大幅消除用户对引导修改的恐惧心理。
    pub fn analyze(original: &GrubConfigFile, draft: &GrubConfigFile) -> ImpactReport {
        let mut items = Vec::new();

        // 1. 分析倒计时变更
        let orig_timeout = original.get("GRUB_TIMEOUT").unwrap_or("5");
        let draft_timeout = draft.get("GRUB_TIMEOUT").unwrap_or("5");
        if orig_timeout != draft_timeout {
            let explanation = if draft_timeout == "0" {
                format!(
                    "开机等待时间由 {} 秒调整为 0 秒，开机时将不显示选择菜单，直接跳入默认启动项。",
                    orig_timeout
                )
            } else {
                format!(
                    "开机等待倒计时由 {} 秒调整为 {} 秒，用户将有更充裕或更精简的时间选择启动项。",
                    orig_timeout, draft_timeout
                )
            };
            items.push(ImpactItem {
                title: "开机倒计时调整".to_string(),
                explanation,
            });
        }

        // 2. 分析默认启动项变更
        let orig_default = original.get("GRUB_DEFAULT").unwrap_or("0");
        let draft_default = draft.get("GRUB_DEFAULT").unwrap_or("0");
        if orig_default != draft_default {
            let explanation = if draft_default == "saved" {
                "默认启动项切换为记忆模式 (saved)，系统将自动记录并默认启动上一次成功进入的系统。"
                    .to_string()
            } else {
                format!(
                    "默认启动项由 '{}' 切换为 '{}'，开机倒计时结束后将默认启动该项。",
                    orig_default, draft_default
                )
            };
            items.push(ImpactItem {
                title: "默认启动项切换".to_string(),
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
            Self::analyze_cmdline_diff(&orig_cmdline, &draft_cmdline, &mut items);
        }

        // 4. 分析 os-prober 探测开关变更
        let orig_prober = original.get("GRUB_DISABLE_OS_PROBER");
        let draft_prober = draft.get("GRUB_DISABLE_OS_PROBER");
        if orig_prober != draft_prober {
            if draft_prober.is_none() || draft_prober == Some("false") {
                items.push(ImpactItem {
                    title: "启用多系统探测 (os-prober)".to_string(),
                    explanation: "生成引导时将自动扫描硬盘上的 Windows 或其他 Linux 系统，并在开机菜单中添加对应条目。".to_string(),
                });
            } else {
                items.push(ImpactItem {
                    title: "禁用多系统探测 (os-prober)".to_string(),
                    explanation: "生成引导时将跳过其他硬盘分区的系统扫描，可加快引导生成速度，但将隐藏其他系统。".to_string(),
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
                        title: "启用开机视觉主题".to_string(),
                        explanation: format!(
                            "开机将应用图形化主题包 '{}'，提供背景壁纸、菜单框与倒计时样式美化。",
                            new_t
                        ),
                    });
                }
                (Some(old_t), None) => {
                    items.push(ImpactItem {
                        title: "禁用开机视觉主题".to_string(),
                        explanation: format!(
                            "清除了主题包 '{}'，开机时将恢复为简单黑底文本菜单。",
                            old_t
                        ),
                    });
                }
                (Some(old_t), Some(new_t)) => {
                    items.push(ImpactItem {
                        title: "切换开机视觉主题".to_string(),
                        explanation: format!("开机主题由 '{}' 切换为 '{}'。", old_t, new_t),
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
                        title: "设置自定义开机壁纸".to_string(),
                        explanation: format!("开机引导背景将显示图片 '{}'。", new_bg),
                    });
                }
                (Some(_), None) => {
                    items.push(ImpactItem {
                        title: "清除自定义开机壁纸".to_string(),
                        explanation: "开机将使用默认单色背景。".to_string(),
                    });
                }
                (Some(old_bg), Some(new_bg)) => {
                    items.push(ImpactItem {
                        title: "更换自定义开机壁纸".to_string(),
                        explanation: format!("壁纸由 '{}' 更换为 '{}'。", old_bg, new_bg),
                    });
                }
                (None, None) => {}
            }
        }

        ImpactReport { items }
    }

    /// 分析内核命令行参数的具体增删语义
    fn analyze_cmdline_diff(orig: &str, draft: &str, items: &mut Vec<ImpactItem>) {
        let orig_tokens: Vec<&str> = orig.split_whitespace().collect();
        let draft_tokens: Vec<&str> = draft.split_whitespace().collect();

        // 检查是否新增了 nomodeset
        if !orig_tokens.contains(&"nomodeset") && draft_tokens.contains(&"nomodeset") {
            items.push(ImpactItem {
                title: "添加通用显示驱动模式 (nomodeset)".to_string(),
                explanation: "内核将在图形驱动加载前使用标准通用驱动，适用于因显卡驱动异常导致的开机黑屏排障。".to_string(),
            });
        }

        // 检查是否移除了 quiet
        if orig_tokens.contains(&"quiet") && !draft_tokens.contains(&"quiet") {
            items.push(ImpactItem {
                title: "关闭静默启动 (移除 quiet)".to_string(),
                explanation: "系统开机时将在屏幕上完整滚屏打印内核加载与服务启动日志，便于精确定位启动卡顿点。".to_string(),
            });
        }

        // 检查是否移除了 splash
        if orig_tokens.contains(&"splash") && !draft_tokens.contains(&"splash") {
            items.push(ImpactItem {
                title: "关闭开机动画 (移除 splash)".to_string(),
                explanation: "开机时将不再显示图形 Logo 或转圈动画，直接呈现文字终端。".to_string(),
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

        let report = ImpactAnalyzer::analyze(&orig, &draft);
        assert_eq!(report.items.len(), 2);
        assert_eq!(report.items[0].title, "开机倒计时调整");
        assert!(report.items[0].explanation.contains("5 秒调整为 10 秒"));
        assert_eq!(report.items[1].title, "默认启动项切换");
        assert!(report.items[1].explanation.contains("记忆模式 (saved)"));
    }

    #[test]
    fn test_impact_kernel_params_nomodeset() {
        let orig = parse_grub_config("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\"\n");
        let draft = parse_grub_config("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash nomodeset\"\n");

        let report = ImpactAnalyzer::analyze(&orig, &draft);
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].title, "添加通用显示驱动模式 (nomodeset)");
        assert!(report.items[0].explanation.contains("显卡驱动异常"));
    }

    #[test]
    fn test_impact_remove_quiet_and_splash() {
        let orig = parse_grub_config("GRUB_CMDLINE_LINUX_DEFAULT=\"quiet splash\"\n");
        let draft = parse_grub_config("GRUB_CMDLINE_LINUX_DEFAULT=\"\"\n");

        let report = ImpactAnalyzer::analyze(&orig, &draft);
        assert_eq!(report.items.len(), 2);
        assert!(report.items.iter().any(|i| i.title.contains("quiet")));
        assert!(report.items.iter().any(|i| i.title.contains("splash")));
    }
}
