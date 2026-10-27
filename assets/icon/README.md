# Tabloom 图标

石墨色圆角底、薄荷绿折角文档及 T 字，独立原创矢量绘制。`Tabloom.png` 是 1024px 预览，`Tabloom.icns` 包含 macOS 常规与 Retina 尺寸。

在 macOS 项目根目录重新生成：

```sh
swift scripts/generate-app-icon.swift /tmp/Tabloom.iconset
iconutil -c icns /tmp/Tabloom.iconset -o assets/icon/Tabloom.icns
cp /tmp/Tabloom.iconset/icon_512x512@2x.png assets/icon/Tabloom.png
```

日常构建直接复制已生成的 icns，无需每次运行 Swift 图标生成器。
