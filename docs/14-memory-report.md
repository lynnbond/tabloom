# 低风险内存优化

日期：2026-10-22。按用户要求优先实施简单、容易验证的内存优化；没有改动存储格式、真实文件写回或渲染后端。

## 已实施

1. `src/editor/highlighting.rs` 使用标准库 OnceLock 延迟初始化语法包和主题。启动、语言菜单、纯文本、空文本以及超限降级显示都不再触发加载。首次显示非空代码时加载一次；语言目录来自固定版本语法包的已验证清单，所有语言均有真实语法测试，缺失时返回错误。纯文本前景色与既有主题一致，有测试比对。
2. `src/ui/editor_region.rs` 的布局缓存直接比较 Galley 已有文本，删除额外保存的完整文本副本。每份缓存原先还会复制一份等长 UTF-8 文本；现在复用既有布局文本。
3. 只保留当前可见文档的自定义绘制布局缓存，切换标签后释放旧标签的缓存。模型文本、egui 编辑状态、撤销栈及语法边界元数据继续保留；回到旧标签时重新生成布局。布局与模型文本相同时直接借用模型，避免为主题/字号等重绘再完整克隆工作区。

没有新增依赖或设置开关。首次使用代码高亮仍需同步加载语法；切回不活动标签会重新布局，这是缓存内存与重绘成本之间的取舍。

## 实测内存

新增 `examples/memory_probe.rs`：真实原生 Glow 窗口、固定 1100×760 逻辑尺寸、临时应用数据目录，12 秒后退出。`scripts/measure-memory-macos.py` 只采样其子进程的 vmmap physical footprint 和 ps RSS，不附加或终止用户现有应用。

macOS ARM64 release 单轮各 3 次采样：

| 内容 | 优化前 physical footprint | 优化后 physical footprint |
| --- | --- | --- |
| 空白纯文本 | 170.3–174.7MiB | 168.7–173.2MiB |
| 一行 Rust 代码 | 170.9–177.6MiB | 170.7–175.2MiB |

原始记录见 [JSON](measurements/memory-macos.json)。RSS 与 physical footprint 是不同口径；不能把两者直接比较。原生字体、图形缓存、系统压缩和后台负载会使短时间采样波动，本次没有证明显著或稳定的启动内存下降，更没有从约 150MB 降到几十 MB。延迟加载只帮助尚未使用高亮的状态，首次高亮之后相关资源仍保留。

多标签缓存优化通过结构测试验证：访问多个工作区后自定义布局缓存始终只有一项，所有工作区的撤销边界元数据仍在；不据此编造整机节省的 MB 数字。egui 自身有短期字体/布局缓存，操作系统也不保证立刻归还全部释放页。

复现方式：

```sh
./scripts/cargo.sh build --release --locked --example memory_probe
python3 scripts/measure-memory-macos.py target/release/examples/memory_probe
python3 scripts/measure-memory-macos.py target/release/examples/memory_probe --syntax rust
```

## 验证

- macOS ARM64：fmt、check（含 all-targets）、89 项 Rust 测试、严格全目标全特性 clippy、release 构建及 `.app` 重新打包通过。
- 新测试验证纯文本/空文本/降级显示不加载资源、首次代码着色初始化、主题颜色一致、标签切换只留一个布局且保留撤销元数据；原有所有语言着色、Unicode、查找、跨区域替换和撤销测试通过。
- macOS 原生连续文档、语言隔离、多标签撤销及正常/异常关闭和备份恢复探针通过。
- Linux ARM64/X11：相同检查、89 项 Rust 测试及 4 项 Python 迁移测试通过；实际启动、连续文档、恢复、文件工作流、旧库迁移、真实 xclip 剪贴板及约 1MiB 编辑/切换/撤销原生探针通过。
- Windows、Linux x86_64、Wayland 尚未实测；CI 未远程执行。原生输入探针仍采用 egui 事件注入。

## 后置项

图形缓冲/Retina 分辨率或渲染后端调整、中文字体裁剪、关闭工作区的按需文本加载、按字节管理撤销历史及大文档虚拟化都需要独立设计和验证。它们涉及显示质量、输入、恢复或撤销语义，本轮保留现有实现。启动图形资源仍占较大比例，本轮不承诺解决整个进程的内存基线。
