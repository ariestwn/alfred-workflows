import AppKit

// Original vector artwork; render once when updating the packaged icon.
let image = NSImage(size: NSSize(width: 256, height: 256))
image.lockFocus()
NSColor(calibratedRed: 0.13, green: 0.17, blue: 0.24, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 8, y: 8, width: 240, height: 240), xRadius: 55, yRadius: 55).fill()
NSColor(calibratedRed: 0.56, green: 0.86, blue: 0.73, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 44, y: 65, width: 168, height: 136), xRadius: 30, yRadius: 30).fill()
let tail = NSBezierPath()
tail.move(to: NSPoint(x: 72, y: 76))
tail.line(to: NSPoint(x: 70, y: 41))
tail.line(to: NSPoint(x: 110, y: 76))
tail.close()
tail.fill()
NSColor(calibratedRed: 0.13, green: 0.17, blue: 0.24, alpha: 1).setStroke()
let prompt = NSBezierPath()
prompt.lineWidth = 10
prompt.lineCapStyle = .round
prompt.lineJoinStyle = .round
prompt.move(to: NSPoint(x: 82, y: 151))
prompt.line(to: NSPoint(x: 103, y: 133))
prompt.line(to: NSPoint(x: 82, y: 115))
prompt.move(to: NSPoint(x: 131, y: 114))
prompt.line(to: NSPoint(x: 168, y: 114))
prompt.stroke()
image.unlockFocus()
let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
