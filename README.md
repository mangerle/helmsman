<div align="center">

<img src="assets/logo-badge.svg" width="380" alt="Helmsman" style="margin-bottom: 12px;" />

**A modern, memory-safe, and reliable GRUB bootloader manager written in pure Rust**

<p align="center">
  <a href="README_zh.md">简体中文</a> | <b>English</b>
</p>

[![Rust](https://img.shields.io/badge/Rust-2024_Edition-orange.svg?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Linux-brightgreen.svg)]()
[![Safety](https://img.shields.io/badge/Safety-Forbid_Unsafe-success.svg)]()
[![Architecture](https://img.shields.io/badge/Architecture-Privilege_Separation-blueviolet.svg)]()

</div>

Helmsman is a modern, memory-safe, and reliable GRUB bootloader configuration tool engineered in Rust. Designed as a robust successor to the classic Grub Customizer, Helmsman provides an intuitive user experience without compromising system stability, security, or distribution upgrade compatibility.

---

## Key Highlights

- **Privilege Separation Architecture**: The graphical interface runs entirely as an unprivileged user process. System modifications are gated behind PolicyKit (polkit) and dispatched through an on-demand D-Bus system service (`helmsman-daemon`).
- **Non-Destructive Boot Sorting**: Unlike legacy tools that corrupt `/etc/grub.d/10_linux` into opaque binary wrappers, Helmsman uses standard, non-destructive mechanisms (such as `GRUB_DEFAULT` semantic indexing and clean custom snippet management). Kernel upgrades (`apt upgrade`, `dnf update`, `pacman -Syu`) remain completely safe.
- **Syntax-Preserving Configuration Engine**: Parses `/etc/default/grub` into a full Abstract Syntax Tree (AST), preserving all existing comments, blank lines, quotation styles, and key ordering.
- **Transactional Safety & Rollback**: Any configuration mutation undergoes Unified Diff generation, atomic temporary file replacement, and automatic snapshot creation, allowing one-click rollback if boot generation fails.
- **Cross-Distribution Native Adaptation**: Out-of-the-box support for Debian/Ubuntu, Fedora/RHEL (both UEFI and BIOS layouts), Arch Linux, and openSUSE.

---

## Comparison: Helmsman vs. Grub Customizer

| Feature | Classic Grub Customizer | Helmsman (Rust) |
| :--- | :--- | :--- |
| **Language** | C++ (GTK3) | 100% Safe Rust |
| **Execution Model** | Entire UI runs as `root` (high security risk) | Privilege separation: Unprivileged UI + Polkit/D-Bus Daemon |
| **Sorting Mechanism** | Overwrites `/etc/grub.d/10_linux` with proxy scripts | Native `GRUB_DEFAULT` indexing & clean `/etc/grub.d/41_*` snippets |
| **Kernel Upgrade Impact**| Frequently breaks boot entries upon kernel updates | Zero interference with distro package manager hooks |
| **`/etc/default/grub` Handling** | Destructive line rewrites; strips comments and order | Full AST preserving comments, empty lines, and quotes |
| **Rollback Capability** | Basic or absent | Atomic file swaps with automatic timestamped snapshots |
| **Distribution Support** | Hardcoded heuristics | Extensible Distro Adapter pattern for all major distros |

---

## Workspace Crates

The project is structured as a modular Cargo workspace:

- **`crates/grub-config-parser`**: Parses and serializes `/etc/default/grub` while preserving line structure, comments, and quoting conventions.
- **`crates/grub-boot-reader`**: Parses `/boot/grub/grub.cfg` to extract the entry hierarchy and introspect kernel paths, initrd images, root UUIDs, boot parameters, and chainloader paths.
- **`crates/grub-distro-adapter`**: Detects and abstracts distribution-specific paths and commands (`update-grub` vs. `grub2-mkconfig`, EFI mount paths, etc.).
- **`crates/grub-transaction-engine`**: Handles atomic writes (`atomic_write`), unified diff calculations, and automated snapshot management.
- **`crates/helmsman-daemon`**: The privileged background worker service, providing transactional configuration application, syntax validation, and D-Bus integration.
- **`crates/helmsman-ui`**: Headless UI state machine and application state management, decoupling business logic and draft workflows from rendering toolkits.

---

## System Integration Files

The `crates/helmsman-daemon/data/` directory contains system integration specifications adhering to freedesktop standards:

- `org.freedesktop.Helmsman.policy`: PolicyKit action definitions for authorized system boot modification.
- `org.freedesktop.Helmsman.service`: D-Bus system bus activation configuration for on-demand daemon startup.
- `helmsman.service`: Systemd unit configuration managing daemon lifecycle.

---

## Building and Testing

### Prerequisites

- Rust 1.96 or later (`cargo`, `rustc`)
- Standard C toolchain (if building system dependencies on Linux)

### Compilation

Clone the repository and build the workspace:

```bash
cargo build --workspace
```

For release builds with Link-Time Optimization (LTO) enabled:

```bash
cargo build --release
```

### Running Tests

All crates include thorough unit and integration test suites:

```bash
cargo test --workspace
```

### Static Analysis and Code Formatting

To ensure compliance with project engineering standards:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
```

---

## License

This project is licensed under the Apache License, Version 2.0 (Apache-2.0). See [LICENSE](LICENSE) for details.
