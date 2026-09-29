import AppKit

func render(_ width: Int, _ height: Int, _ path: String, _ draw: () -> Void) throws {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    draw()
    NSGraphicsContext.restoreGraphicsState()
    try rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
}

let out = CommandLine.arguments[1]
if CommandLine.arguments.contains("--fixture") {
    try render(3840, 2160, out) {
        NSColor(calibratedWhite: 0.96, alpha: 1).setFill()
        NSRect(x: 0, y: 0, width: 3840, height: 2160).fill()
        for row in 0..<20 {
            NSColor(calibratedRed: CGFloat(row % 5) * 0.1 + 0.15, green: 0.45, blue: 0.65, alpha: 1).setFill()
            NSBezierPath(roundedRect: NSRect(x: 180, y: row * 94 + 80, width: 2800 - row * 30, height: 48), xRadius: 8, yRadius: 8).fill()
        }
        ("Generated 4K test screenshot" as NSString).draw(at: NSPoint(x: 180, y: 1990),
            withAttributes: [.font: NSFont.systemFont(ofSize: 72, weight: .bold), .foregroundColor: NSColor.black])
    }
} else {
    for name in ["icon", "previous", "next"] {
        try render(256, 256, "\(out)/\(name).png") {
            NSColor(calibratedRed: 0.17, green: 0.39, blue: 0.64, alpha: 1).setFill()
            NSBezierPath(roundedRect: NSRect(x: 12, y: 12, width: 232, height: 232), xRadius: 48, yRadius: 48).fill()
            NSColor.white.setStroke()
            let line = NSBezierPath()
            line.lineWidth = 11
            line.lineCapStyle = .round
            line.lineJoinStyle = .round
            if name == "icon" {
                for (x, y, dx, dy) in [(58, 58, 1, 1), (198, 58, -1, 1), (58, 198, 1, -1), (198, 198, -1, -1)] {
                    line.move(to: NSPoint(x: x, y: y + dy * 35))
                    line.line(to: NSPoint(x: x, y: y))
                    line.line(to: NSPoint(x: x + dx * 35, y: y))
                }
                line.appendRoundedRect(NSRect(x: 94, y: 94, width: 68, height: 68), xRadius: 8, yRadius: 8)
            } else {
                let sign = name == "next" ? 1 : -1
                line.move(to: NSPoint(x: 128 - sign * 52, y: 128))
                line.line(to: NSPoint(x: 128 + sign * 52, y: 128))
                line.move(to: NSPoint(x: 128 + sign * 12, y: 172))
                line.line(to: NSPoint(x: 128 + sign * 56, y: 128))
                line.line(to: NSPoint(x: 128 + sign * 12, y: 84))
            }
            line.stroke()
        }
    }
}
