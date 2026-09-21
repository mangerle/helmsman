/// Polkit 权限动作定义（与 org.freedesktop.Helmsman.policy 一一对应）
pub mod polkit_actions {
    /// 读取系统引导状态与配置
    pub const ACTION_READ: &str = "org.freedesktop.Helmsman.read";
    /// 快速切换默认启动项（仅修改 grubenv）
    pub const ACTION_SET_DEFAULT: &str = "org.freedesktop.Helmsman.set-default";
    /// 提交配置变更并重新编译引导脚本
    pub const ACTION_APPLY_CHANGES: &str = "org.freedesktop.Helmsman.apply-changes";
    /// 回滚配置至指定历史快照
    pub const ACTION_ROLLBACK: &str = "org.freedesktop.Helmsman.rollback";
    /// 修改条目友好别名映射
    pub const ACTION_SET_ALIAS: &str = "org.freedesktop.Helmsman.set-alias";
    /// 安装 GRUB 主题压缩包
    pub const ACTION_INSTALL_THEME: &str = "org.freedesktop.Helmsman.install-theme";
}
