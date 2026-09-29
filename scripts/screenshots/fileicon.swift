// Writes the Finder icon of each path as a PNG, like Alfred's {"type":"fileicon"}.
// Input on stdin: a JSON array of {"path": ..., "out": ...}.
import AppKit

let size = 128
let data = FileHandle.standardInput.readDataToEndOfFile()
let jobs = try JSONSerialization.jsonObject(with: data) as! [[String: String]]
for job in jobs {
    let icon = NSWorkspace.shared.icon(forFile: job["path"]!)
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    icon.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
    NSGraphicsContext.restoreGraphicsState()
    try rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: job["out"]!))
}
