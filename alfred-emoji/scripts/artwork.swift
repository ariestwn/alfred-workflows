import AppKit

func render(_ size: Int, _ path: String, _ draw: (CGFloat) -> Void) throws {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    draw(CGFloat(size))
    NSGraphicsContext.restoreGraphicsState()
    try rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: path))
}

func glyph(_ text: String, _ size: CGFloat, _ scale: CGFloat) {
    let font = NSFont(name: "Apple Color Emoji", size: size * scale)!
    let string = NSAttributedString(string: text, attributes: [.font: font])
    let bounds = string.boundingRect(with: NSSize(width: size * 2, height: size * 2), options: [.usesLineFragmentOrigin])
    string.draw(at: NSPoint(x: (size - bounds.width) / 2 - bounds.minX, y: (size - bounds.height) / 2 - bounds.minY))
}

func fileName(_ emoji: String) -> String {
    emoji.unicodeScalars.map { String($0.value, radix: 16) }.joined(separator: "-")
}

let root = CommandLine.arguments[1]
let out = "\(root)/workflow/icons"
try FileManager.default.createDirectory(atPath: out, withIntermediateDirectories: true)
let rows = try String(contentsOfFile: "\(root)/data/emoji.tsv", encoding: .utf8).split(separator: "\n")
for row in rows {
    let emoji = String(row.split(separator: "\t")[0])
    try render(128, "\(out)/\(fileName(emoji)).png") { glyph(emoji, $0, 0.78) }
}
try render(256, "\(root)/workflow/icon.png") { size in
    NSColor(calibratedRed: 0.99, green: 0.78, blue: 0.2, alpha: 1).setFill()
    NSBezierPath(roundedRect: NSRect(x: 12, y: 12, width: size - 24, height: size - 24), xRadius: 52, yRadius: 52).fill()
    glyph("😀", size, 0.6)
}
print("\(rows.count) emoji icons rendered")
