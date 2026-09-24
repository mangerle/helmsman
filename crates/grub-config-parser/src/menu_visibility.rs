//! 菜单类级可见性开关（恢复模式 / os-prober / 子菜单折叠）的类型化契约。
//!
//! # 设计原理
//! - **实现初衷**：类级隐藏有原生 GRUB 配置键支撑（`GRUB_DISABLE_RECOVERY` 等），
//!   零侵入且 update-grub 后稳定；单项隐藏没有标准接口，明确后置，不在本模块承诺。
//! - **核心优势**：三类开关收敛为布尔策略对象，写回时语义清晰（true 禁用 / 移除键即启用）。
//! - **代价与局限**：仅覆盖类级开关；单条 menuentry 的隐藏/删除不在本契约范围内。

use crate::ast::GrubConfigFile;

/// 菜单类级可见性策略
///
/// 三个开关互相独立，均可单独启停：
/// - `recovery_disabled`：隐藏发行版生成的救援/恢复模式条目
/// - `os_prober_disabled`：隐藏 os-prober 探测到的其他系统条目
/// - `submenu_disabled`：拉平二级子菜单（`GRUB_DISABLE_SUBMENU=y`）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuVisibility {
    /// 是否隐藏恢复模式条目
    pub recovery_disabled: bool,
    /// 是否禁用 os-prober（隐藏其他系统条目）
    pub os_prober_disabled: bool,
    /// 是否禁用子菜单折叠（拉平菜单）
    pub submenu_disabled: bool,
}

impl MenuVisibility {
    /// 从配置读取当前类级可见性策略
    pub fn from_config(config: &GrubConfigFile) -> Self {
        Self {
            recovery_disabled: config.get("GRUB_DISABLE_RECOVERY") == Some("true"),
            os_prober_disabled: config.get("GRUB_DISABLE_OS_PROBER") == Some("true"),
            submenu_disabled: config.get("GRUB_DISABLE_SUBMENU") == Some("y"),
        }
    }

    /// 将策略写回配置（保留未改动键的原有引号与注释）
    ///
    /// 语义：禁用时写入对应键；启用时移除键以恢复发行版默认行为。
    pub fn apply_to(self, config: &mut GrubConfigFile) {
        if self.recovery_disabled {
            config.set("GRUB_DISABLE_RECOVERY", "true");
        } else {
            config.remove("GRUB_DISABLE_RECOVERY");
        }

        if self.os_prober_disabled {
            config.set("GRUB_DISABLE_OS_PROBER", "true");
        } else {
            config.remove("GRUB_DISABLE_OS_PROBER");
        }

        if self.submenu_disabled {
            config.set("GRUB_DISABLE_SUBMENU", "y");
        } else {
            config.remove("GRUB_DISABLE_SUBMENU");
        }
    }

    /// 是否存在任一类被隐藏
    pub fn has_any_hidden(self) -> bool {
        self.recovery_disabled || self.os_prober_disabled || self.submenu_disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_grub_config;

    #[test]
    fn test_read_visibility_from_config() {
        let config = parse_grub_config(
            "GRUB_DISABLE_RECOVERY=true\nGRUB_DISABLE_OS_PROBER=true\nGRUB_DISABLE_SUBMENU=y\n",
        );
        let vis = MenuVisibility::from_config(&config);
        assert!(vis.recovery_disabled);
        assert!(vis.os_prober_disabled);
        assert!(vis.submenu_disabled);
        assert!(vis.has_any_hidden());
    }

    #[test]
    fn test_default_visibility_all_shown() {
        let config = parse_grub_config("GRUB_DEFAULT=0\n");
        let vis = MenuVisibility::from_config(&config);
        assert_eq!(vis, MenuVisibility::default());
        assert!(!vis.has_any_hidden());
    }

    #[test]
    fn test_apply_hides_and_restores() {
        let mut config = parse_grub_config("# 头注释\nGRUB_TIMEOUT=5\n");

        let hidden = MenuVisibility {
            recovery_disabled: true,
            os_prober_disabled: true,
            submenu_disabled: true,
        };
        hidden.apply_to(&mut config);
        let text = config.serialize();
        assert!(text.contains("GRUB_DISABLE_RECOVERY=\"true\""));
        assert!(text.contains("GRUB_DISABLE_OS_PROBER=\"true\""));
        assert!(text.contains("GRUB_DISABLE_SUBMENU=\"y\""));
        assert!(text.contains("# 头注释"));
        assert!(text.contains("GRUB_TIMEOUT=5"));

        // 恢复显示后对应键应被移除
        MenuVisibility::default().apply_to(&mut config);
        let restored = config.serialize();
        assert!(!restored.contains("GRUB_DISABLE_RECOVERY"));
        assert!(!restored.contains("GRUB_DISABLE_OS_PROBER"));
        assert!(!restored.contains("GRUB_DISABLE_SUBMENU"));
        assert!(restored.contains("GRUB_TIMEOUT=5"));
    }

    #[test]
    fn test_partial_visibility() {
        let mut config = parse_grub_config("GRUB_DEFAULT=0\n");
        let only_recovery = MenuVisibility {
            recovery_disabled: true,
            ..Default::default()
        };
        only_recovery.apply_to(&mut config);
        assert!(config.get("GRUB_DISABLE_RECOVERY") == Some("true"));
        assert!(config.get("GRUB_DISABLE_OS_PROBER").is_none());
        assert!(config.get("GRUB_DISABLE_SUBMENU").is_none());
    }
}
