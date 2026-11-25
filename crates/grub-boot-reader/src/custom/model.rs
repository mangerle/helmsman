use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 自定义脚本标准头部定义
pub const HELMSMAN_CUSTOM_HEADER: &str =
    "#!/bin/sh\nexec tail -n +3 $0\n# 本文件由 Helmsman 自动生成与维护，请勿手动修改头部声明\n";

/// 自定义引导项类型常量
pub mod entry_types {
    /// Live ISO 镜像 loopback 引导
    pub const ISO: &str = "iso";
    /// 链式加载器引导（Windows / 独立 EFI）
    pub const CHAINLOADER: &str = "chainloader";
    /// 自定义 Linux 内核引导
    pub const LINUX: &str = "linux";
    /// 原生自定义脚本代码块
    pub const RAW: &str = "raw";
}

/// 自定义引导项安全校验错误
///
/// # 设计原理
/// - **实现初衷**：自定义条目最终会被拼进受管的 `/etc/grub.d/41_helmsman_custom`，
///   任一字段逃逸都可能导致 GRUB 脚本注入，必须在生成脚本前强类型拒绝。
/// - **核心优势**：错误变体带字段名与原因，便于上层边界直接映射为用户可读信息。
/// - **代价与局限**：对标题、路径等采用保守白名单，牺牲少量非常规字符兼容以换取确定安全性。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CustomEntryError {
    /// 条目标识不符合命名规范
    #[error("条目标识 '{id}' 非法，原因: {reason}")]
    InvalidId { id: String, reason: String },
    /// 菜单标题包含危险字符
    #[error("条目标题 '{title}' 非法，原因: {reason}")]
    InvalidTitle { title: String, reason: String },
    /// 条目类型不受支持
    #[error("条目类型 '{entry_type}' 不受支持，允许值: iso / chainloader / linux / raw")]
    InvalidEntryType { entry_type: String },
    /// 样式分类标签非法
    #[error("样式分类 '{class}' 非法，原因: {reason}")]
    InvalidClass { class: String, reason: String },
    /// 目标路径或 initrd 路径非法
    #[error("路径 '{path}' 非法，原因: {reason}")]
    InvalidPath { path: String, reason: String },
    /// 分区 UUID 非法
    #[error("分区 UUID '{uuid}' 非法，原因: {reason}")]
    InvalidPartitionUuid { uuid: String, reason: String },
    /// 内核命令行参数非法
    #[error("内核启动参数 '{cmdline}' 非法，原因: {reason}")]
    InvalidCmdline { cmdline: String, reason: String },
    /// 原生脚本内容非法
    #[error("原生脚本非法，原因: {reason}")]
    InvalidRawScript { reason: String },
    /// 条目列表存在冲突（ID 或标题重复）
    #[error("条目列表存在冲突，原因: {reason}")]
    Conflict { reason: String },
    /// 生成脚本结构非法
    #[error("生成的自定义脚本结构非法，原因: {reason}")]
    InvalidScriptStructure { reason: String },
}

/// 单个自定义引导条目领域模型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomBootEntry {
    /// 唯一标识 ID
    pub id: String,
    /// 菜单显示标题
    pub title: String,
    /// 样式图标标签分类（例如 ["ubuntu", "os"], ["windows", "os"]）
    pub classes: Vec<String>,
    /// 条目分类 ("iso" / "chainloader" / "linux" / "raw")
    pub entry_type: String,
    /// 目标路径（ISO 路径 / EFI 路径 / 内核镜像路径）
    pub target_path: String,
    /// 目标分区 UUID（存放 ISO / EFI / 根分区的 UUID）
    pub partition_uuid: String,
    /// 初始内存盘路径（initrd，仅 linux 类型使用）
    pub initrd_path: String,
    /// 内核启动命令行参数
    pub cmdline_params: String,
    /// 原生脚本代码内容（仅 raw 类型使用）
    pub raw_script: String,
    /// 是否启用该引导条目（若禁用则生成时自动注释掉）
    pub enabled: bool,
    /// 所属子菜单标题（空字符串表示位于菜单根层级）
    pub submenu_title: String,
}

impl CustomBootEntry {
    /// 创建 ISO 镜像引导条目
    pub fn new_iso_boot(
        id: impl Into<String>,
        title: impl Into<String>,
        isofile_path: impl Into<String>,
        partition_uuid: impl Into<String>,
        extra_cmdline: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            classes: vec!["gnu-linux".to_string(), "os".to_string()],
            entry_type: entry_types::ISO.to_string(),
            target_path: isofile_path.into(),
            partition_uuid: partition_uuid.into(),
            initrd_path: String::new(),
            cmdline_params: extra_cmdline.into(),
            raw_script: String::new(),
            enabled: true,
            submenu_title: String::new(),
        }
    }

    /// 创建 Windows / EFI 链式加载引导条目
    pub fn new_chainloader(
        id: impl Into<String>,
        title: impl Into<String>,
        efi_path: impl Into<String>,
        partition_uuid: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            classes: vec!["windows".to_string(), "os".to_string()],
            entry_type: entry_types::CHAINLOADER.to_string(),
            target_path: efi_path.into(),
            partition_uuid: partition_uuid.into(),
            initrd_path: String::new(),
            cmdline_params: String::new(),
            raw_script: String::new(),
            enabled: true,
            submenu_title: String::new(),
        }
    }

    /// 创建自定义 Linux 内核引导条目
    pub fn new_custom_linux(
        id: impl Into<String>,
        title: impl Into<String>,
        kernel_path: impl Into<String>,
        initrd_path: impl Into<String>,
        root_uuid: impl Into<String>,
        cmdline: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            classes: vec!["gnu-linux".to_string(), "os".to_string()],
            entry_type: entry_types::LINUX.to_string(),
            target_path: kernel_path.into(),
            partition_uuid: root_uuid.into(),
            initrd_path: initrd_path.into(),
            cmdline_params: cmdline.into(),
            raw_script: String::new(),
            enabled: true,
            submenu_title: String::new(),
        }
    }

    /// 创建通用原生脚本引导条目
    pub fn new_raw_script(
        id: impl Into<String>,
        title: impl Into<String>,
        script_content: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            classes: vec!["os".to_string()],
            entry_type: entry_types::RAW.to_string(),
            target_path: String::new(),
            partition_uuid: String::new(),
            initrd_path: String::new(),
            cmdline_params: String::new(),
            raw_script: script_content.into(),
            enabled: true,
            submenu_title: String::new(),
        }
    }

    /// 将条目移入指定子菜单（空标题表示移回根层级）
    pub fn with_submenu(mut self, submenu_title: impl Into<String>) -> Self {
        self.submenu_title = submenu_title.into();
        self
    }
}
