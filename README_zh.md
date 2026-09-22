<div align="center">

<img src="assets/logo-badge.svg" width="380" alt="Helmsman" style="margin-bottom: 12px;" />

**基于纯 Rust 打造的现代化、内存安全且可靠的 Linux GRUB 引导管理工具**

<p align="center">
  <b>简体中文</b> | <a href="README.md">English</a>
</p>

[![Rust](https://img.shields.io/badge/Rust-2024_Edition-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Linux-brightgreen.svg)]()
[![Safety](https://img.shields.io/badge/Safety-Forbid_Unsafe-success.svg)]()
[![Architecture](https://img.shields.io/badge/Architecture-Privilege_Separation-blueviolet.svg)]()

</div>

Helmsman 是一个基于 Rust 语言构建的现代化、内存安全且可靠的 Linux GRUB 引导配置工具。作为经典工具 Grub Customizer 的强健继任者，Helmsman 提供无头 UI 状态机与特权分离守护进程基础能力；桌面 GUI 前端仍在规划中，当前仓库尚未包含可启动的图形程序。

---

## 核心特性

- **特权分离架构**：图形界面完全运行在普通非特权用户进程中。所有系统级修改均受 PolicyKit (polkit) 鉴权保护，并通过 D-Bus 系统总线按需激活后台服务 (`helmsman-daemon`)，平时不常驻后台消耗内存。
- **无损引导排序**：彻底弃用传统工具将 `/etc/grub.d/10_linux` 篡改为二进制代理脚本的破坏性做法。Helmsman 采用原生标准的 `GRUB_DEFAULT` 语义索引与规范的自定义段管理，系统执行内核升级（如 `apt upgrade`、`dnf update`、`pacman -Syu`）时绝对安全。
- **语法保真配置引擎**：将 `/etc/default/grub` 解析为抽象语法树 (AST)，回写时 100% 保留原有注释、空白行、引号风格及键值排列顺序，杜绝破坏性覆盖。
- **事务性安全与快照回滚**：任何配置变更均提供 Unified Diff 差异预览、临时文件原子替换 (`atomic_write`) 以及带时间戳的自动快照备份，若引导文件生成失败可实现一键秒级回滚。
- **跨发行版原生适配**：内置对 Debian/Ubuntu、Fedora/RHEL（支持 UEFI 与 BIOS 差异布局）、Arch Linux 以及 openSUSE 的路径与生成命令自动探测与适配。

---

## 方案对比：Helmsman 与传统 Grub Customizer

| 功能特性 | 经典 Grub Customizer | Helmsman |
| :--- | :--- | :--- |
| **开发语言** | C++ (GTK3) | 100% 内存安全 Rust |
| **运行模式** | 整个 UI 必须以 `root` 权限启动（极高安全隐患） | 特权分离：非特权 UI + Polkit/D-Bus 后台服务 |
| **排序机制** | 将 `/etc/grub.d/10_linux` 劫持为二进制代理脚本 | 原生 `GRUB_DEFAULT` 逻辑索引与受管 `/etc/grub.d/41_*` 脚本 |
| **内核升级影响** | 发行版更新内核时频繁损坏引导菜单或引发黑屏 | 零侵入，对发行版官方包管理器钩子完全透明 |
| **配置解析回写** | 粗暴覆盖文件，丢失用户注释、空白行与原有排版 | 语法保真 AST，完整保留所有注释、空白行与引号风格 |
| **灾难回滚能力** | 基础或缺失，一旦出错难以排查恢复 | 事务引擎：原子替换写入，失败自动恢复，快照归档 |
| **发行版兼容性** | 依靠硬编码逻辑推断 | 基于 Distro Adapter 适配器模式，精准支持各大主流发行版 |

---

## 工作区模块构成

本项目采用标准 Cargo Workspace 模块化架构组织：

- **`crates/grub-config-parser`**：负责 `/etc/default/grub` 的 AST 解析与语法保真回写，保留注释与格式。
- **`crates/grub-boot-reader`**：负责解析 `/boot/grub/grub.cfg` 树形结构，反解析提取内核镜像路径、initrd、根分区 UUID、启动参数及链式加载器。
- **`crates/grub-distro-adapter`**：跨发行版环境适配器，负责探测并抽象不同 Linux 发行版的引导配置文件路径与生成命令。
- **`crates/grub-transaction-engine`**：事务执行引擎，负责 Unified Diff 差异计算、临时文件原子安全替换 (`atomic_write`) 与快照管理。
- **`crates/helmsman-client`**：非特权 D-Bus 契约层，提供 DTO、Polkit 动作常量与系统总线类型安全代理，供 UI 与第三方工具消费。
- **`crates/helmsman-daemon`**：特权守护进程，提供配置更新的事务执行、环境校验及 D-Bus 系统总线接口。
- **`crates/helmsman-ui`**：Headless UI 状态机与草稿数据模型，将业务交互逻辑与界面渲染层解耦。

---

## 系统集成配置

在 `crates/helmsman-daemon/data/` 目录下提供了符合 freedesktop 规范的系统集成文件：

- `org.freedesktop.Helmsman.policy`：PolicyKit 授权策略文件，定义普通用户修改引导时的管理员鉴权规则。
- `org.freedesktop.Helmsman.service`：D-Bus 系统总线按需激活配置文件。
- `helmsman.service`：systemd 服务单元文件，配合 D-Bus 实现生命周期自托管。

---

## 编译与测试

### 环境要求

- Rust 1.96 或更高版本 (`cargo`, `rustc`)
- 标准 C 构建工具链（Linux 环境下依赖）

### 编译构建

克隆本仓库并在工作区根目录执行：

```bash
cargo build --workspace
```

构建启用 LTO（链接时优化）与符号裁剪的高性能发布版本：

```bash
cargo build --release
```

### 运行自动化测试

所有模块均包含完整的单元测试与集成测试套件：

```bash
cargo test --workspace --features helmsman-daemon/test-support  # 需在启用 test-support 的 crate 上执行，或分别指定 -p
```

### 代码格式与静态检查

项目遵循严格的工程质量门禁：

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

---

## 开源许可证

本项目基于 Apache 2.0 许可证 (Apache-2.0) 开源发布。详见 [LICENSE](LICENSE) 文件。
