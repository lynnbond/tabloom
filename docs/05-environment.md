# 第一步：环境检查记录

检查日期：2026-10-21。目录：`/Users/liyan/git/tabloom`。

以下为第一步执行时的历史快照；第二步已新增 Cargo 项目与应用源码，最新状态见 [第二步记录](06-step2-report.md)。

## 目录与规范

- 初始目录仅有 `.cargo-home/` 与 `.rustup-home/`，没有 Cargo.toml、Cargo.lock、src 或现有应用代码。
- 当前目录不是 Git 仓库，`git status` 返回 `not a git repository`；本轮未初始化 Git、未提交代码。
- 未发现当前项目的 AGENTS.md 文件；遵循会话提供的规范。检查的 `/Users/liyan/git/AGENTS.md`、`/Users/liyan/AGENTS.md` 不存在。
- 当前暴露工具中没有 codebase-memory-mcp 的索引或搜索工具，无法调用图谱。项目无源码，本轮仅用文件系统检查，没有需要追踪的代码调用关系。

## 工具链实测

| 项目 | 实测结果 |
| --- | --- |
| 系统 | macOS 26.6.2，Darwin arm64 |
| rustc | `rustc 1.99.1 (58cf8f9c3 2026-10-13)` |
| Cargo | `cargo 1.99.1 (5f94df478 2026-08-27)` |
| rustfmt | `rustfmt 1.10.0-stable (58cf8f9c35 2026-10-13)` |
| clippy | `clippy 0.1.99 (58cf8f9c35 2026-10-13)` |
| 已安装 target | 仅 `aarch64-apple-darwin` |
| 已安装组件 | cargo、rustc、rust-std、rustfmt、clippy |
| C 工具链 | Apple clang 17.0.0，`/usr/bin/clang` |
| 开发工具目录 | `/Library/Developer/CommandLineTools` |
| SDK | `/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk` |
| 可用空间 | 约 520GiB（检查时快照） |

默认 shell 的 PATH 找不到 `rustc` 和 `cargo`，但项目内的工具链可用，不需要重新下载安装。后续在项目根目录的当前终端设置：

```sh
export CARGO_HOME="$PWD/.cargo-home"
export RUSTUP_HOME="$PWD/.rustup-home"
export PATH="$CARGO_HOME/bin:$PATH"
rustc --version
cargo --version
```

这是当前环境的工具链使用方式，不应成为未来用户安装 Tabloom 的前提；正式 README 应提供标准 Rust 安装方式。不得提交本地 `.cargo-home/`、`.rustup-home/`、target、数据库或用户内容。

## 编译与运行冒烟验证

使用临时目录中的独立 Rust 程序，调用本地 rustc 编译、链接并执行，成功输出：

```text
Tabloom Rust/linker smoke OK
```

临时文件已自动清理，没有向项目写入应用源码。此结果证明本机 Rust 编译、系统链接器和简单二进制执行可用；不代表 eframe、SQLite、系统文件对话框或剪贴板集成已验证。

## 平台与当前限制

- 可开展 macOS ARM64 原生构建验证。
- Windows/Linux target 和原生构建环境当前未配置；需要后续对应 runner 或桌面环境，不能声称已验证这两个平台。
- 没有 Cargo 项目，因此尚不能执行项目级 fmt/check/test/clippy，也不存在可启动的 Tabloom 应用。
- 当前检查没有操作图形桌面，不能据此确认后续 GUI 自动化或手动交互能力。
- 依赖版本、下载可用性、平台 feature 和原生库尚待第二步核实，不将工具链冒烟成功等同于依赖可用。

## 结论与下一步

第一步已完成：空项目状态明确，本机工具链与链接器可用，环境路径问题已有不修改全局配置的解决方式。

下一步进入核心骨架：初始化 Cargo 项目，选择并验证实际依赖版本，实现能启动、可输入的多标签多区域编辑器。本轮按最新指令止于环境检查，不提前执行第二步。
