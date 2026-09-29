import AppKit
let root = URL(fileURLWithPath: CommandLine.arguments[1])
try FileManager.default.createDirectory(at: root.appendingPathComponent("icons"), withIntermediateDirectories: true)
func save(_ name: String, symbol: String, color: NSColor, background: Bool = false) throws {
    let image = NSImage(size: NSSize(width: 256, height: 256))
    image.lockFocus()
    if background {
        NSColor(calibratedRed: 0.08, green: 0.30, blue: 0.62, alpha: 1).setFill()
        NSBezierPath(roundedRect: NSRect(x: 8, y: 8, width: 240, height: 240), xRadius: 52, yRadius: 52).fill()
    }
    let config = NSImage.SymbolConfiguration(pointSize: 145, weight: .medium).applying(.init(paletteColors: [color]))
    if let glyph = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?.withSymbolConfiguration(config) {
        let factor = min(174 / glyph.size.width, 174 / glyph.size.height)
        let size = NSSize(width: glyph.size.width * factor, height: glyph.size.height * factor)
        glyph.draw(in: NSRect(x: (256-size.width)/2, y: (256-size.height)/2, width: size.width, height: size.height))
    }
    image.unlockFocus()
    let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
    try bitmap.representation(using: .png, properties: [:])!.write(to: root.appendingPathComponent(name))
}
try save("icon.png", symbol: "calendar.badge.clock", color: .white, background: true)
for (name, symbol, color) in [
    ("event", "calendar", NSColor.systemBlue),
    ("accepted", "checkmark.circle", NSColor.systemBlue),
    ("pending", "questionmark.circle", NSColor.systemBlue),
    ("tentative", "clock", NSColor.systemOrange),
    ("declined", "xmark.circle", NSColor.systemGray),
    ("info", "circle.dotted", NSColor.systemGray)
] { try save("icons/\(name).png", symbol: symbol, color: color) }
