# 唯一应用入口与图标

原先 `target/debug/Tabloom.app`、`target/release/Tabloom.app`、`~/Applications/Tabloom.app` 三个同名包都登记到 Launch Services。打包脚本默认改为 release 并安装到用户 Applications，重复运行只更新这个入口。`--install` 保留为显式参数。需要独立开发包时使用 `debug --local`，名称为 Tabloom Dev，bundle ID 为 `org.tabloom.Tabloom.dev`，与日常应用区分。

安装时取消旧 debug/release 同名包的 Launch Services 登记，再移动到 `target/retired-apps/<临时标识>/Tabloom.app.disabled`，不删除旧构建文件或工作区。历史临时启动探针的过期登记也已移除。本轮实际检查登记表只剩 `~/Applications/Tabloom.app`。

图标采用原创矢量绘制，包含石墨色圆角背景、薄荷绿折角文档和 T 字。矢量源是 `scripts/generate-app-icon.swift`；`assets/icon/Tabloom.icns` 提供完整 macOS 多尺寸图标，Info.plist 的 CFBundleIconFile 指向它。1024px 预览和再生成说明位于 `assets/icon/`。

验证：图标生成器运行、iconutil 转换、shell 语法、release 构建、plist 校验均通过；NSWorkspace 实际读取安装包图标，并导出检查确认其不是默认图标。再次执行默认打包仍只更新已安装包。Finder 已定位唯一应用。未修改 Rust 编辑/持久化代码，未重复执行全量 Rust 测试，未重启 Finder/Dock 或关闭用户进程。

## 运行时 Dock 图标修复

应用包图标与运行中的 Dock 图标曾不一致：eframe 0.37 在 viewport.icon 未指定时会加载框架默认图标，并在 macOS 运行时调用 NSApplication 的图标设置覆盖应用包图标。现在 `src/main.rs` 在 macOS 显式设置空 IconData，使用该版本框架支持的“保留应用原有图标”行为，使 Finder 与 Dock 使用同一份应用包图标。其他平台显式加载相同设计的 PNG。

本轮 macOS 全目标编译、103 项测试、格式检查和严格 clippy 通过。release 更新仍原子替换可执行文件，不退出正在运行的用户进程；用户正常退出后重新打开新版即可使用修复。Windows/Linux 本轮没有重新运行验证，运行中的旧实例不会被热更新。
