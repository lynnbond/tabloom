# macOS 系统顶栏菜单

macOS 的正式启动入口现在安装原生 AppKit 菜单，并隐藏窗口内的重复菜单栏。工作区、编辑、外观及窗口菜单位于屏幕顶栏；Tabloom 应用菜单提供关于、隐藏、隐藏其他应用、显示全部、退出。Windows/Linux 继续使用现有 egui 窗口菜单。

## 实现

- `src/ui/native_menu.rs`：固定菜单、动态最近关闭/永久删除/区域定位菜单、主题勾选、可用状态和事件桥接。仅 macOS 编译，在主线程使用 muda 0.21.2 的 AppKit 后端。依赖只在 macOS 启用，关闭默认 GTK 特性，Cargo.lock 固定解析版本。
- `src/app.rs`：正式应用主动安装原生菜单；菜单事件调用与原菜单相同的 Action，不直接改模型或访问持久化。退出通过现有窗口关闭和保存检查；复制/粘贴仍经纯文本编辑命令处理。菜单事件请求 egui 重绘唤醒事件循环。动态菜单按标识/标题变化更新，不读取或复制全文。模态确认期间禁用相应菜单项。
- `src/main.rs`：正式启动时安装菜单。安装或更新失败会显示错误并恢复窗口菜单，不阻止编辑器启动。测试默认构造不连接全局 AppKit 菜单，独立原生探针显式安装。
- `src/ui/text.rs`：新增系统菜单文案集中管理。

原生快捷键先由 AppKit 菜单处理，`performKeyEquivalent` 返回已消费；未处理的键继续交给现有 egui 快捷键。测试发现 AppKit 的 Shift 字符等价键需大写字符，重做与替换因此使用 `Z`/`H` 逻辑等价键，保持 ⌘⇧Z、⌘⇧H；⌘H 仍用于系统隐藏。应用内 Ctrl+Tab 导航保留。

菜单框架参考：[muda 官方文档](https://docs.rs/muda/0.21.2/muda/)；实际 API 同时依据本地锁定版本源码核对。

## 实测

- macOS ARM64：`cargo fmt --all -- --check`、`cargo check --all-targets --locked`、103 项 `cargo test --locked` 和 `cargo clippy --all-targets --all-features --locked -- -D warnings` 全部通过；release 已更新至 `~/Applications/Tabloom.app`。
- `cargo run --locked --example native_menu_smoke` 通过：从真实 NSApplication.mainMenu 查找菜单项并通过 AppKit 操作；验证新建仅执行一次、关闭/重新打开、全选、撤销/重做、查找、替换、主题、保存、退出及磁盘恢复。替换快捷键未触发隐藏。模拟磁盘写入失败时退出被取消、其他菜单禁用，恢复写入后正常退出。
- 原生探针只使用临时工作区；不关闭用户当前实例，不修改正式文档。菜单选择及快捷键由 AppKit API 注入，不等同于实体键鼠和中文输入法的人工验收。
- Linux ARM64/X11：完整容器回归通过，包括格式/编译/103 项测试/严格 clippy，以及四主题、目录选择、主程序启动、保存/崩溃恢复、文件冲突、真实 X11 剪贴板和 1 MiB 文本探针。未引入 GTK 菜单依赖。
- Windows、Linux x86_64、Wayland 本轮没有实际运行。

已有实例仍运行旧代码；正常退出后双击用户 Applications 中的 Tabloom 即可看到系统菜单。更新没有生成新的同名应用入口。
