# macOS 原生应用启动

本轮排查发现运行中的 Tabloom 来自 `target/release/tabloom`，连接终端 TTY。它是 Unix 裸可执行文件；Finder 双击这类文件会交给终端运行。已有 `.app` 的入口直接指向 Mach-O，没有通过 shell 启动终端。

`./scripts/macos-app.sh` 现在默认构建 release，增加 `--install`，将原生应用包安装到 `~/Applications/Tabloom.app` 并注册 Launch Services。用户通过 Finder、Spotlight 或 Dock 打开 `.app`。项目内应用包构建仍保留。更新可执行文件仍采用原子替换，避免影响正在运行的旧进程。

本次已实际安装并重新生成项目 release 包。README 将 macOS 日常使用入口放在开发命令之前，并说明不要双击裸二进制。

验证：shell 语法检查、release 构建和两个 plist 校验通过；使用安装包的副本替换为现有原生文档探针，通过 `open -n -W` / Launch Services 打开，原生窗口与隔离数据恢复均通过，前后 Terminal 进程集合相同。探针使用临时目录，没有启动第二个正式工作区实例、退出用户旧实例或修改用户文档。此轮仅改打包脚本与说明，没有修改 Rust 业务代码，未重复运行全量 Rust 测试。
