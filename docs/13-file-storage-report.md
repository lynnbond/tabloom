# 文件存储实施记录

历史记录：其中的旧格式兼容、迁移流程和阻止启动策略已经取消，当前行为见 [非阻塞启动修复](16-startup-report.md)。

日期：2026-10-22。按用户要求先更新 [设计](12-file-storage-design.md)，再替换实现。当前 [架构](02-architecture.md)、[需求](01-requirements.md) 和根 README 已同步。

## 完成内容

- 移除 Cargo.toml/Cargo.lock 中的 rusqlite、libsqlite3-sys 及相关依赖；删除原 `database.rs`、`migrations.rs`、`recovery.rs`。`cargo tree --locked -e normal` 无 SQLite 依赖。
- `src/persistence/store.rs` 实现内容寻址工作区 JSON、版本化 `session.json` 清单、原子发布、SHA-256 校验、完整模型校验、独占实例锁、完整备份、损坏归档恢复和旧版本清理。
- `Persistence` 继续负责防抖、最长等待、失败重试和备份/维护错误；已去掉 WAL 检查点。
- `AppPaths`、UI 数据路径和错误文案切换为文件语义。真实文件自动保存隔离、同源片段完整写回、Ctrl/Cmd+S 与导出规则保持原有行为。
- `scripts/migrate-sqlite.py` 独立使用 Python 标准库只读迁移 schema 1/2；支持已提交 WAL、保持 UUID、文本、顺序、语法、布局兼容字段、文件关联及偏好。应用本身不导入 Python、不链接 SQLite，日常使用无需 Python。
- 发现旧库、孤立旧 WAL/SHM、旧恢复标记或旧备份时，拒绝创建空白状态，显示迁移指引；新格式不会重复导入迁移快照。
- macOS release `.app` 已重新打包，附带迁移脚本；原生探针、Linux 验证和 CI 已更新。

## 实测验证

macOS ARM64：格式、`cargo check --locked`、`cargo check --all-targets --locked`、87 项 Rust 测试、严格全目标全特性 clippy、release 构建和 `.app` 打包通过。`scripts/test_migrate_sqlite.py` 的 4 项测试通过，含 schema 1/2、原始文本/关联、已提交 WAL、实例锁、未知版本、已有新数据及损坏保护。

Linux ARM64 Debian bookworm/X11：容器中 fmt/check/test/clippy、构建及 87 项 Rust 测试、4 项 Python 测试通过。实际 ELF 初始启动创建 JSON 工作区；Xvfb/Mesa 连续文档恢复、正常与异常退出恢复、损坏后备份恢复、文件导入导出与冲突、真实 xclip 剪贴板、约 1MiB 编辑/切换/撤销重做探针通过。

macOS 原生窗口已验证：连续文档保存重载、正常关闭重启、故意损坏临时清单后恢复备份、跳过全部退出析构后恢复已提交清单，以及关联 UTF-16 文件保存/导出和外部修改冲突。所有探针使用临时目录，不读取正式工作区。

`python3 scripts/verify-native-migration.py` 生成两个临时旧库，真实运行迁移 CLI，再启动 Rust 原生窗口导入并再次重启；逐字段比较整个 Snapshot，确认文本、中文、Emoji、混合换行、ID、语法、文件关联、偏好均不变，原数据库字节保留。macOS 与 Linux ARM64/X11 均已通过。

Windows、Linux x86_64、Wayland 尚未实际构建/运行。本次不宣称降低了启动内存，未对新旧版本内存作受控对照。图形编辑探针使用 egui 事件注入，不等于实体键盘/输入法验收。CI 配置未推送或远程执行。

## 失败与一致性覆盖

自动化包括：最终清单替换失败仍能读到上次完整状态、dirty 与错误提示保留、修复路径后重试；未变化工作区引用复用；关闭/永久删除不破坏其他标签；备份轮转后仍独立可恢复；缺失/损坏的工作区恢复；损坏最新备份跳过；未知新版本和普通 I/O 错误不进行破坏性恢复；无备份时归档并拒绝空白重置；孤立新文件/旧 WAL 不当作首次启动；越界引用拒绝；已有旧库迁移提交一次后不重复导入；内部保存失败阻止真实文件写回但允许应急导出。

目录故障测试是真实文件系统操作，不依赖 mock 返回成功。未制造真实磁盘耗尽或硬件断电，不能把这些用例等同于断电/满盘实机验收。

## 性能与限制

一次 macOS release 冒烟测量：3 个工作区，各约 1MiB 大范围与两个小范围；首次完整 JSON 保存约 46.52ms，读取约 3.67ms。原始结果见 [测量 JSON](measurements/file-storage-macos.json)。这是首次写入全部文本且同步文件的路径，不代表每次按键，也不是统计基准。相较之前 SQLite 的单次测量没有更快；本次收益是去除数据库依赖、数据可检查和存储流程简化，不能推导性能或内存降低。

备份包含全部内容，每 60 秒至多一次；强杀会丢失尚未提交的防抖窗口，损坏后从备份恢复可能回退最多一个备份周期甚至更久（若备份失败）。Windows 未实现父目录 fsync。工作区 JSON 具有校验和，不支持在应用外直接编辑后自动接受变更。迁移要求关闭旧应用，并一次性提供 Python 3；旧库损坏需先处理旧备份。清理错误可能暂时留下旧修订，归档及迁移源不会自动删除。

## 启动与已有数据

当前正式数据目录和已运行旧进程没有被自动改写或终止。首次使用无旧数据时直接启动；有旧数据时先正常退出旧应用，再执行：

```sh
python3 scripts/migrate-sqlite.py --data-dir "$HOME/Library/Application Support/org.tabloom.Tabloom"
open target/release/Tabloom.app
```

原 SQLite、WAL 和备份保留。确认新版本恢复正确后，可以另行归档旧文件；应用不会自动删除它们。其他系统路径见根 README。
