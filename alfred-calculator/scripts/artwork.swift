import AppKit
let image = NSImage(size: NSSize(width: 256, height: 256))
image.lockFocus()
NSColor(calibratedRed: 0.22, green: 0.23, blue: 0.48, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 6, y: 6, width: 244, height: 244), xRadius: 54, yRadius: 54).fill()
NSColor(calibratedWhite: 1, alpha: 0.13).setFill()
NSBezierPath(roundedRect: NSRect(x: 36, y: 151, width: 184, height: 63), xRadius: 15, yRadius: 15).fill()
let number = "123" as NSString
number.draw(at: NSPoint(x: 114, y: 160), withAttributes: [.font: NSFont.monospacedDigitSystemFont(ofSize: 36, weight: .medium), .foregroundColor: NSColor.white])
for (x, y, label) in [(42, 89, "+"), (139, 89, "−"), (42, 32, "×"), (139, 32, "=")] {
    let selected = label == "="
    (selected ? NSColor(calibratedRed: 0.54, green: 0.91, blue: 0.75, alpha: 1) : NSColor(calibratedWhite: 1, alpha: 0.12)).setFill()
    NSBezierPath(roundedRect: NSRect(x: x, y: y, width: 75, height: 46), xRadius: 12, yRadius: 12).fill()
    (label as NSString).draw(at: NSPoint(x: x+26, y: y+4), withAttributes: [.font: NSFont.systemFont(ofSize: 31, weight: .medium), .foregroundColor: selected ? NSColor(calibratedRed: 0.12, green: 0.20, blue: 0.25, alpha: 1) : NSColor.white])
}
image.unlockFocus()
let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
