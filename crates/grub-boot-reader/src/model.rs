/// 单个引导条目
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootEntry {
    /// 条目显示名称（例如："Ubuntu"、"Windows Boot Manager (on /dev/nvme0n1p1)"）
    pub title: String,
    /// 唯一标识 ID（例如："gnulinux-simple-..."）
    pub id: Option<String>,
    /// 完整层级访问路径（例如："Advanced options for Ubuntu>Ubuntu, with Linux 6.8.0-40-generic"）
    pub full_path: String,
    /// 分类标签（例如：["ubuntu", "gnu-linux", "os"]）
    pub classes: Vec<String>,
    /// 内核映像路径（例如："/boot/vmlinuz-6.8.0-40-generic"）
    pub kernel_path: Option<String>,
    /// 初始内存盘路径（例如："/boot/initrd.img-6.8.0-40-generic"）
    pub initrd_path: Option<String>,
    /// 根分区 UUID（例如："5452f864-16a7-47b2-bdcf-cc7728ef28ab"）
    pub root_uuid: Option<String>,
    /// 启动参数命令行（例如："ro quiet splash $vt_handoff"）
    pub cmdline_params: Option<String>,
    /// 链式引导路径（例如："/EFI/Microsoft/Boot/bootmgfw.efi"）
    pub chainloader_path: Option<String>,
}

/// 引导菜单层级节点
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuNode {
    /// 普通引导条目
    Entry(BootEntry),
    /// 子菜单（包含子项列表）
    Submenu {
        /// 子菜单标题（例如："Advanced options for Ubuntu"）
        title: String,
        /// 子菜单唯一标识 ID
        id: Option<String>,
        /// 完整层级路径
        full_path: String,
        /// 子菜单下的节点
        children: Vec<MenuNode>,
    },
}

impl MenuNode {
    /// 扁平化获取当前节点及其子树下的所有叶子引导条目
    ///
    /// # 设计原理
    /// - **实现初衷**：为前端条目列表展示、检索与逻辑排序提供扁平化的引导条目视图。
    /// - **核心优势**：采用显式堆栈（`Vec<&MenuNode>`）迭代遍历代替深度递归，彻底规避极端嵌套情况下的栈溢出隐患；
    ///   直接向共享容器追加借用引用，避免多次创建中间临时容器的堆分配开销。
    /// - **代价与局限**：遍历顺序为深度优先先序遍历，符合 GRUB 菜单默认从上至下的视觉呈现次序。
    pub fn collect_entries(&self) -> Vec<&BootEntry> {
        let mut entries = Vec::with_capacity(8);
        self.collect_entries_into(&mut entries);
        entries
    }

    /// 将当前节点及其子树下的叶子条目高效追加至指定的引用容器中
    pub fn collect_entries_into<'a>(&'a self, entries: &mut Vec<&'a BootEntry>) {
        let mut stack = Vec::with_capacity(8);
        stack.push(self);

        while let Some(current) = stack.pop() {
            match current {
                MenuNode::Entry(entry) => entries.push(entry),
                MenuNode::Submenu { children, .. } => {
                    // 逆序入栈以保证出栈顺序与原始子项顺序严格一致
                    for child in children.iter().rev() {
                        stack.push(child);
                    }
                }
            }
        }
    }
}
