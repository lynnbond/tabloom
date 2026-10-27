// Vector source for Tabloom's macOS icon. Run on macOS to regenerate assets/icon/Tabloom.icns.
import AppKit
import Foundation

let output = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "assets/icon/Tabloom.iconset"
try FileManager.default.createDirectory(atPath: output, withIntermediateDirectories: true)

func color(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat) -> NSColor {
    NSColor(srgbRed: r / 255, green: g / 255, blue: b / 255, alpha: 1)
}
func rounded(_ rect: NSRect, _ radius: CGFloat, _ fill: NSColor) {
    fill.setFill()
    NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius).fill()
}
func polygon(_ points: [NSPoint], _ fill: NSColor) {
    let path = NSBezierPath()
    path.move(to: points[0])
    for point in points.dropFirst() { path.line(to: point) }
    path.close()
    fill.setFill()
    path.fill()
}
func render(_ pixels: Int, _ filename: String) throws {
    let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
    let context = NSGraphicsContext.current!.cgContext
    context.scaleBy(x: CGFloat(pixels) / 1024, y: CGFloat(pixels) / 1024)
    rounded(NSRect(x: 64, y: 64, width: 896, height: 896), 200, color(25, 31, 33))
    let paper = NSBezierPath()
    paper.move(to: NSPoint(x: 290, y: 224))
    paper.line(to: NSPoint(x: 734, y: 224))
    paper.line(to: NSPoint(x: 734, y: 650))
    paper.line(to: NSPoint(x: 584, y: 800))
    paper.line(to: NSPoint(x: 290, y: 800))
    paper.close()
    color(158, 212, 191).setFill()
    paper.fill()
    polygon([NSPoint(x:584,y:800), NSPoint(x:584,y:650), NSPoint(x:734,y:650)], color(83, 143, 121))
    // A clear T and two text lines, readable down to the 16pt Finder icon.
    rounded(NSRect(x: 365, y: 570, width: 195, height: 46), 8, color(25, 47, 40))
    rounded(NSRect(x: 440, y: 410, width: 46, height: 198), 8, color(25, 47, 40))
    rounded(NSRect(x: 365, y: 331, width: 294, height: 22), 10, color(77, 133, 113))
    rounded(NSRect(x: 365, y: 285, width: 206, height: 22), 10, color(77, 133, 113))
    NSGraphicsContext.restoreGraphicsState()
    let data = bitmap.representation(using: .png, properties: [:])!
    try data.write(to: URL(fileURLWithPath: output).appendingPathComponent(filename))
}
for size in [16, 32, 128, 256, 512] {
    try render(size, "icon_\(size)x\(size).png")
    try render(size * 2, "icon_\(size)x\(size)@2x.png")
}
