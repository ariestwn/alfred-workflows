// Apple Music Audio Watcher — menubar app + background log watcher.
//
// Combines:
//   1. Tails the unified log for Music.app's MediaToolbox format report,
//      mirrors the live audio format into a JSON cache so the Alfred
//      workflow keywords (`np`, `nplog`, …) keep working.
//   2. Hosts a menubar item that opens a rich Now Playing popover —
//      artwork, title, artist, progress, transport controls — plus a
//      right-click actions menu for format switch / auto-apply / hide /
//      Alfred bridge / quit.

import AppKit
import Combine
import CoreAudio
import Foundation
import SwiftUI

// MARK: - Paths -

let label = "com.ariestwn.apple-music-audio-format"
let home = ProcessInfo.processInfo.environment["HOME"] ?? NSHomeDirectory()
let supportDir = "\(home)/Library/Application Support/\(label)"
let cacheDir = "\(home)/Library/Caches/\(label)"
let cachePath = "\(cacheDir)/nowplaying.json"
let applyLogPath = "\(cacheDir)/apply.log"
let offSwitchPath = "\(supportDir)/autoapply.off"
let menubarOffPath = "\(supportDir)/menubar.off"
let displayModePath = "\(supportDir)/display.mode"
let artworkPath = "\(cacheDir)/artwork.jpg"
// Music.app stores album art for the now-playing track (including streaming
// content) in this URL cache. Files are JPEGs named with UUIDs.
let musicArtworkDir = "\(home)/Library/Caches/com.apple.Music/MusicUIArtworkCache/fsCachedData"

try? FileManager.default.createDirectory(
    atPath: cacheDir, withIntermediateDirectories: true)

// MARK: - CoreAudio helpers -

let kMain = AudioObjectPropertyElement(kAudioObjectPropertyElementMain)
let kSystem = AudioObjectID(kAudioObjectSystemObject)

struct DeviceFormat: Hashable {
    let rate: Int
    let bits: UInt32
    let isFloat: Bool

    var label: String {
        let kind = isFloat ? "Float" : "Integer"
        let khz = Double(rate) / 1000.0
        let rateStr = khz == khz.rounded()
            ? "\(Int(khz)) kHz" : String(format: "%.1f kHz", khz)
        return "\(bits)-bit \(kind) · \(rateStr)"
    }
}

func defaultOutputDevice() -> AudioDeviceID? {
    var id: AudioDeviceID = 0
    var size = UInt32(MemoryLayout<AudioDeviceID>.size)
    var addr = AudioObjectPropertyAddress(
        mSelector: kAudioHardwarePropertyDefaultOutputDevice,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kMain)
    let s = AudioObjectGetPropertyData(kSystem, &addr, 0, nil, &size, &id)
    return s == noErr && id != 0 ? id : nil
}

func deviceName(_ d: AudioDeviceID) -> String {
    var addr = AudioObjectPropertyAddress(
        mSelector: kAudioObjectPropertyName,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kMain)
    var n: Unmanaged<CFString>?
    var size = UInt32(MemoryLayout<CFString?>.size)
    if AudioObjectGetPropertyData(d, &addr, 0, nil, &size, &n) == noErr {
        return n?.takeRetainedValue() as String? ?? "Unknown"
    }
    return "Unknown"
}

func outputStreams(_ d: AudioDeviceID) -> [AudioStreamID] {
    var addr = AudioObjectPropertyAddress(
        mSelector: kAudioDevicePropertyStreams,
        mScope: kAudioDevicePropertyScopeOutput,
        mElement: kMain)
    var size: UInt32 = 0
    guard AudioObjectGetPropertyDataSize(d, &addr, 0, nil, &size) == noErr else { return [] }
    var streams = [AudioStreamID](
        repeating: 0, count: Int(size) / MemoryLayout<AudioStreamID>.size)
    return AudioObjectGetPropertyData(d, &addr, 0, nil, &size, &streams) == noErr
        ? streams : []
}

func availableFormats(_ stream: AudioStreamID) -> [AudioStreamBasicDescription] {
    var addr = AudioObjectPropertyAddress(
        mSelector: kAudioStreamPropertyAvailablePhysicalFormats,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kMain)
    var size: UInt32 = 0
    guard AudioObjectGetPropertyDataSize(stream, &addr, 0, nil, &size) == noErr else { return [] }
    var ranges = [AudioStreamRangedDescription](
        repeating: AudioStreamRangedDescription(),
        count: Int(size) / MemoryLayout<AudioStreamRangedDescription>.size)
    guard AudioObjectGetPropertyData(stream, &addr, 0, nil, &size, &ranges) == noErr
    else { return [] }
    return ranges.map { $0.mFormat }
}

func currentFormat(_ stream: AudioStreamID) -> AudioStreamBasicDescription? {
    var addr = AudioObjectPropertyAddress(
        mSelector: kAudioStreamPropertyPhysicalFormat,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kMain)
    var fmt = AudioStreamBasicDescription()
    var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
    return AudioObjectGetPropertyData(stream, &addr, 0, nil, &size, &fmt) == noErr ? fmt : nil
}

@discardableResult
func setFormat(_ stream: AudioStreamID, _ fmt: AudioStreamBasicDescription) -> Bool {
    var f = fmt
    var addr = AudioObjectPropertyAddress(
        mSelector: kAudioStreamPropertyPhysicalFormat,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kMain)
    return AudioObjectSetPropertyData(
        stream, &addr, 0, nil,
        UInt32(MemoryLayout<AudioStreamBasicDescription>.size), &f) == noErr
}

func formatKey(_ f: AudioStreamBasicDescription) -> DeviceFormat {
    DeviceFormat(
        rate: Int(f.mSampleRate.rounded()),
        bits: f.mBitsPerChannel,
        isFloat: (f.mFormatFlags & kAudioFormatFlagIsFloat) != 0)
}

func applyAudioFormat(rate: Int, bits: UInt32) -> Bool {
    guard let device = defaultOutputDevice() else { return false }
    let streams = outputStreams(device)
    guard let firstStream = streams.first else { return false }

    let candidates = availableFormats(firstStream)
    let preferredBits: [UInt32] = [bits, 24, 32, 16].uniqued()
    for tryBits in preferredBits {
        if let match = candidates.first(where: {
            Int($0.mSampleRate.rounded()) == rate
                && $0.mBitsPerChannel == tryBits
                && ($0.mFormatFlags & kAudioFormatFlagIsFloat) == 0
        }) {
            var allOK = true
            for s in streams { if !setFormat(s, match) { allOK = false } }
            return allOK
        }
    }
    return false
}

extension Array where Element: Hashable {
    func uniqued() -> [Element] {
        var seen = Set<Element>()
        return filter { seen.insert($0).inserted }
    }
}

// MARK: - Artwork fetch (iTunes Search API) -

/// Look up the artwork URL for the now-playing track via Apple's public
/// iTunes Search endpoint, then download a small JPEG thumbnail. Public
/// endpoint, no auth, returns the same mzstatic.com URLs Music.app uses
/// internally. One request per track change.
func fetchArtwork(name: String, artist: String) {
    guard !name.isEmpty else { return }
    var c = URLComponents(string: "https://itunes.apple.com/search")!
    c.queryItems = [
        URLQueryItem(name: "term", value: "\(name) \(artist)"),
        URLQueryItem(name: "entity", value: "song"),
        URLQueryItem(name: "limit", value: "1"),
        URLQueryItem(name: "country", value: "us"),
    ]
    guard let searchURL = c.url else { return }

    URLSession.shared.dataTask(with: searchURL) { data, _, _ in
        guard let data = data,
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let results = obj["results"] as? [[String: Any]],
              let first = results.first,
              let small = first["artworkUrl100"] as? String
        else { return }

        // mzstatic URLs use a `<W>x<H>bb.jpg` size segment. Bumping to 200×200
        // gives a Retina-sharp 64pt thumbnail without bloating bandwidth.
        let url200 = small.replacingOccurrences(of: "100x100bb", with: "200x200bb")
        guard let imgURL = URL(string: url200) else { return }

        URLSession.shared.dataTask(with: imgURL) { imgData, _, _ in
            guard let d = imgData, d.count > 1_000 else { return }
            // .atomic writes to a tmp file then renames — handles both the
            // first-write and subsequent overwrites without the
            // replaceItemAt "no original" failure mode.
            try? d.write(to: URL(fileURLWithPath: artworkPath), options: .atomic)
        }.resume()
    }.resume()
}

// MARK: - Now playing model -

struct FormatInfo {
    var codec: String     // ALAC / AAC / etc.
    var rate: Int?
    var bits: Int?
    var rendition: String

    var line: String {
        var parts: [String] = []
        if let r = rate {
            let khz = Double(r) / 1000.0
            parts.append(khz == khz.rounded()
                ? "\(Int(khz)) kHz" : String(format: "%.1f kHz", khz))
        }
        if let b = bits, b > 0 { parts.append("\(b)-bit") }
        if !codec.isEmpty { parts.append(codec) }
        switch rendition {
        case "Lossless", "Hi-Res Lossless", "Dolby Atmos", "Apple Digital Master":
            parts.append(rendition)
        default: break
        }
        return parts.joined(separator: " · ")
    }
}

func mapCodec(_ raw: String) -> String {
    switch raw.lowercased() {
    case "qlac", "alac": return "ALAC"
    case "qaac", "aac", "aach", "aacp": return "AAC"
    case "lpcm", "pcm": return "PCM"
    case "flac": return "FLAC"
    default: return raw
    }
}

// MARK: - Log watcher -

final class LogWatcher {
    var onChange: (() -> Void)?
    private var lastApplied = ""
    private var buffer = ""
    private var task: Process?

    private let formatPat = #/\[AudioFormat ([a-zA-Z0-9]+)/#
    private let rendPat = #/\[Rendition ([^\]]+)\]/#
    private let ratePat = #/\[SampleRate (\d+)\]/#
    private let bitsPat = #/\[BitDepth (\d+)\]/#
    private let chnPat = #/\[AudioChannels (\d+)\]/#

    private let dateFormatter: DateFormatter = {
        let f = DateFormatter(); f.dateFormat = "yyyy-MM-dd HH:mm:ss"; return f
    }()

    var isRunning: Bool { task?.isRunning == true }

    func start() {
        if isRunning { return }
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/log")
        p.arguments = [
            "stream", "--info", "--style", "compact",
            "--predicate", #"process == "Music" AND senderImagePath CONTAINS "MediaToolbox""#,
        ]
        let pipe = Pipe()
        p.standardOutput = pipe
        p.standardError = FileHandle.nullDevice
        pipe.fileHandleForReading.readabilityHandler = { [weak self] h in
            let data = h.availableData
            guard !data.isEmpty, let s = String(data: data, encoding: .utf8) else { return }
            self?.feed(s)
        }
        try? p.run()
        self.task = p
    }

    func stop() {
        task?.terminate()
        task = nil
        buffer = ""
        lastApplied = ""
    }

    private func feed(_ chunk: String) {
        buffer.append(chunk)
        while let nl = buffer.firstIndex(of: "\n") {
            let line = String(buffer[..<nl])
            buffer.removeSubrange(buffer.startIndex...nl)
            handle(line)
        }
    }

    private func cap(_ line: String, _ pat: Regex<(Substring, Substring)>) -> String {
        (try? pat.firstMatch(in: line)).map { String($0.1) } ?? ""
    }

    private func handle(_ line: String) {
        guard line.contains("ReportAudioPlaybackThroughFigLog") else { return }

        let fmt = cap(line, formatPat)
        let rend = cap(line, rendPat)
        let rate = cap(line, ratePat)
        let bits = cap(line, bitsPat)
        let chn = cap(line, chnPat)
        let ts = Int(Date().timeIntervalSince1970)

        var payload: [String: Any] = [
            "timestamp": ts,
            "format": fmt,
            "rendition": rend,
            "source": "report",
        ]
        payload["sampleRate"] = Int(rate) ?? NSNull()
        payload["bitDepth"] = Int(bits) ?? NSNull()
        payload["channels"] = Int(chn) ?? NSNull()
        if let data = try? JSONSerialization.data(withJSONObject: payload) {
            // .atomic handles both first-write and overwrite cleanly,
            // unlike replaceItemAt which fails when the target is missing.
            try? data.write(to: URL(fileURLWithPath: cachePath), options: .atomic)
        }

        let key = "\(rate)-\(bits)"
        if key != lastApplied,
           !rate.isEmpty, !bits.isEmpty,
           !FileManager.default.fileExists(atPath: offSwitchPath),
           let r = Int(rate), let b = UInt32(bits)
        {
            if applyAudioFormat(rate: r, bits: b) {
                let line = "\(dateFormatter.string(from: Date())) applied \(rate)/\(bits)-bit int\n"
                if let d = line.data(using: .utf8) {
                    if let h = FileHandle(forWritingAtPath: applyLogPath) {
                        defer { try? h.close() }
                        _ = try? h.seekToEnd()
                        try? h.write(contentsOf: d)
                    } else {
                        try? d.write(to: URL(fileURLWithPath: applyLogPath))
                    }
                }
                lastApplied = key
            }
        }
        DispatchQueue.main.async { self.onChange?() }
    }
}

// MARK: - AppleScript player bridge -

enum PlayerScript {
    static func run(_ src: String) -> String {
        var err: NSDictionary?
        return NSAppleScript(source: src)?
            .executeAndReturnError(&err).stringValue ?? ""
    }
    static func void(_ src: String) {
        var err: NSDictionary?
        NSAppleScript(source: src)?.executeAndReturnError(&err)
    }
    static func playPause() {
        void(#"tell application "Music" to playpause"#)
    }
    static func next() { void(#"tell application "Music" to next track"#) }
    static func prev() { void(#"tell application "Music" to previous track"#) }
    // Music.app's `shuffle enabled` and `song repeat` properties became
    // effectively read-only in recent versions — setting them via AppleScript
    // accepts the value but doesn't apply it. UI-scripting via System Events
    // is the only reliable path. Run the script via /usr/bin/osascript
    // subprocess instead of in-process NSAppleScript: from a launchd-spawned
    // accessory daemon, in-process AppleScript can't reach Music.app's menu
    // bar items, but a fresh osascript subprocess inherits the user's GUI
    // session and works.
    // Cache: only prompt for Accessibility once per daemon lifetime so
    // repeated shuffle/repeat clicks don't spam the system dialog.
    nonisolated(unsafe) private static var didPromptAX = false

    private static func ensureAccessibility() {
        if AXIsProcessTrusted() { return }
        if didPromptAX { return }
        didPromptAX = true
        let opts = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
        _ = AXIsProcessTrustedWithOptions(opts)
    }

    /// Run AppleScript in-process. Critical for UI scripting via System
    /// Events: macOS checks Accessibility against the *running process's*
    /// executable. Spawning /usr/bin/osascript would attribute the request
    /// to osascript (which the user can't grant — it lives on the read-only
    /// system volume). NSAppleScript runs inside our binary, so the
    /// Accessibility grant on our .app applies.
    private static func runScript(_ src: String) {
        ensureAccessibility()
        var err: NSDictionary?
        NSAppleScript(source: src)?.executeAndReturnError(&err)
    }
    static func toggleShuffle() {
        runScript("""
        tell application "System Events"
            tell process "Music"
                set m to menu 1 of menu item "Shuffle" of menu 1 of menu bar item "Controls" of menu bar 1
                set onMI to menu item "On" of m
                set offMI to menu item "Off" of m
                if (value of attribute "AXMenuItemMarkChar" of onMI) is "✓" then
                    click offMI
                else
                    click onMI
                end if
            end tell
        end tell
        """)
    }
    static func cycleRepeat() {
        runScript("""
        tell application "System Events"
            tell process "Music"
                set m to menu 1 of menu item "Repeat" of menu 1 of menu bar item "Controls" of menu bar 1
                set offMI to menu item "Off" of m
                set allMI to menu item "All" of m
                set oneMI to menu item "One" of m
                if (value of attribute "AXMenuItemMarkChar" of offMI) is "✓" then
                    click allMI
                else if (value of attribute "AXMenuItemMarkChar" of allMI) is "✓" then
                    click oneMI
                else
                    click offMI
                end if
            end tell
        end tell
        """)
    }
    static func openAlfred(_ kw: String) {
        void(#"tell application id "com.runningwithcrayons.Alfred" to search "\#(kw)""#)
    }
}

struct PlayerSnapshot: Equatable {
    var name: String = ""
    var artist: String = ""
    var album: String = ""
    var isPlaying: Bool = false
    var hasTrack: Bool = false
    var position: Double = 0
    var duration: Double = 0
    var shuffle: Bool = false
    var repeatMode: String = "off"  // off / one / all
    var permissionDenied: Bool = false
}

func fetchPlayerSnapshot() -> PlayerSnapshot {
    var snap = PlayerSnapshot()
    let metaScript = """
    if application "Music" is not running then return ""
    tell application "Music"
        if player state is stopped then return "STOPPED"
        try
            set t to current track
            set n to name of t
            set ar to artist of t
            set al to album of t
            set ps to player state as string
            set pp to (player position as string)
            set du to (duration of t) as string
            set sh to ""
            try
                set sh to (shuffle enabled as string)
            end try
            set rp to ""
            try
                set rp to (song repeat as string)
            end try
            return n & tab & ar & tab & al & tab & ps & tab & pp & tab & du & tab & sh & tab & rp
        on error
            return ""
        end try
    end tell
    """
    var err: NSDictionary?
    let result = NSAppleScript(source: metaScript)?.executeAndReturnError(&err)
    if let e = err, (e[NSAppleScript.errorNumber] as? Int) == -1743 {
        snap.permissionDenied = true
        return snap
    }
    guard let s = result?.stringValue, !s.isEmpty, s != "STOPPED" else { return snap }
    let p = s.components(separatedBy: "\t")
    guard p.count >= 6 else { return snap }
    snap.hasTrack = true
    snap.name = p[0]
    snap.artist = p[1]
    snap.album = p[2]
    snap.isPlaying = p[3] == "playing"
    // AppleScript returns numbers using the system's decimal separator —
    // on locales like id_ID that's a comma. Normalize before parsing so
    // Swift's Double initializer (which is locale-agnostic) accepts it.
    snap.position = Double(p[4].replacingOccurrences(of: ",", with: ".")) ?? 0
    snap.duration = Double(p[5].replacingOccurrences(of: ",", with: ".")) ?? 0
    if p.count >= 7 { snap.shuffle = p[6] == "true" }
    if p.count >= 8 { snap.repeatMode = p[7] }
    return snap
}

// MARK: - Player store (SwiftUI observable) -

final class PlayerStore: ObservableObject {
    @Published var snapshot = PlayerSnapshot()
    @Published var format = FormatInfo(codec: "", rate: nil, bits: nil, rendition: "")
    @Published var deviceLabel: String = ""
    @Published var artwork: NSImage?
    private var timer: Timer?
    private var lastTrackID: String = ""
    private var lastArtworkMtime: Date?

    func startPolling() {
        refresh()
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.refresh()
        }
        if let t = timer { RunLoop.main.add(t, forMode: .common) }
    }

    func stopPolling() {
        timer?.invalidate(); timer = nil
    }

    func refresh() {
        DispatchQueue.global(qos: .userInitiated).async {
            let snap = fetchPlayerSnapshot()
            let fmt = self.readFormatCache()
            let dev = self.readDevice()

            // Track-change detection: when name/artist differs from last
            // known, fetch a fresh artwork via iTunes Search.
            let trackKey = "\(snap.name)::\(snap.artist)"
            if snap.hasTrack && trackKey != self.lastTrackID {
                self.lastTrackID = trackKey
                fetchArtwork(name: snap.name, artist: snap.artist)
            }

            let (art, mtime) = self.readArtworkIfChanged()
            DispatchQueue.main.async {
                if snap != self.snapshot { self.snapshot = snap }
                self.format = fmt
                self.deviceLabel = dev
                if let a = art { self.artwork = a; self.lastArtworkMtime = mtime }
                else if !FileManager.default.fileExists(atPath: artworkPath) {
                    self.artwork = nil
                }
            }
        }
    }

    private func readArtworkIfChanged() -> (NSImage?, Date?) {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: artworkPath),
              let mtime = attrs[.modificationDate] as? Date
        else { return (nil, nil) }
        if mtime == lastArtworkMtime { return (nil, mtime) }  // unchanged
        guard let img = NSImage(contentsOfFile: artworkPath) else { return (nil, mtime) }
        return (img, mtime)
    }

    private func readFormatCache() -> FormatInfo {
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: cachePath)),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return FormatInfo(codec: "", rate: nil, bits: nil, rendition: "") }
        return FormatInfo(
            codec: mapCodec(obj["format"] as? String ?? ""),
            rate: obj["sampleRate"] as? Int,
            bits: obj["bitDepth"] as? Int,
            rendition: obj["rendition"] as? String ?? "")
    }

    private func readDevice() -> String {
        guard let d = defaultOutputDevice() else { return "" }
        let n = deviceName(d)
        let streams = outputStreams(d)
        if let s = streams.first, let f = currentFormat(s) {
            return "\(n)  ·  \(formatKey(f).label)"
        }
        return n
    }
}

// MARK: - SwiftUI now-playing card -

struct NowPlayingCard: View {
    @ObservedObject var store: PlayerStore
    var onActions: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if store.snapshot.permissionDenied {
                permissionCard
            } else if !store.snapshot.hasTrack {
                emptyCard
            } else {
                playingCard
            }
            Divider().padding(.vertical, 8)
            footer
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .frame(width: 360)
    }

    var permissionCard: some View {
        VStack(spacing: 6) {
            Image(systemName: "lock.shield").font(.title)
            Text("Music access denied").font(.headline)
            Text("Grant permission in System Settings → Privacy & Security → Automation.")
                .font(.caption).multilineTextAlignment(.center).foregroundStyle(.secondary)
            Button("Open Privacy Settings") {
                if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation") {
                    NSWorkspace.shared.open(url)
                }
            }
        }
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .center)
    }

    var emptyCard: some View {
        VStack(spacing: 6) {
            Image(systemName: "music.note").font(.title).foregroundStyle(.secondary)
            Text("Apple Music: not playing").font(.headline)
        }
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .center)
    }

    @ViewBuilder
    var playingCard: some View {
        HStack(alignment: .top, spacing: 12) {
            artwork
                .frame(width: 64, height: 64)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(Color.primary.opacity(0.08), lineWidth: 1))
            VStack(alignment: .leading, spacing: 3) {
                Text(store.snapshot.name)
                    .font(.system(size: 14, weight: .semibold))
                    .lineLimit(1)
                    .truncationMode(.tail)
                Text(store.snapshot.artist)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if !store.snapshot.album.isEmpty {
                    Text(store.snapshot.album)
                        .font(.system(size: 11))
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
                if !store.format.line.isEmpty {
                    Text(store.format.line)
                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .padding(.top, 2)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
        }
        progress.padding(.top, 10)
        controls.padding(.top, 8)
    }

    @ViewBuilder
    var artwork: some View {
        if let img = store.artwork {
            Image(nsImage: img).resizable().aspectRatio(contentMode: .fill)
        } else {
            ZStack {
                Color.secondary.opacity(0.15)
                Image(systemName: "music.note").foregroundStyle(.secondary)
            }
        }
    }

    var progress: some View {
        VStack(spacing: 2) {
            GeometryReader { geo in
                let frac = store.snapshot.duration > 0
                    ? store.snapshot.position / store.snapshot.duration : 0
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.primary.opacity(0.12)).frame(height: 4)
                    Capsule().fill(Color.primary.opacity(0.6))
                        .frame(width: geo.size.width * frac, height: 4)
                }
            }
            .frame(height: 4)
            HStack {
                Text(formatTime(store.snapshot.position))
                Spacer()
                Text("-" + formatTime(max(0, store.snapshot.duration - store.snapshot.position)))
            }
            .font(.system(size: 10, design: .monospaced))
            .foregroundStyle(.secondary)
        }
    }

    var controls: some View {
        HStack {
            controlButton(systemName: store.snapshot.shuffle ? "shuffle.circle.fill" : "shuffle",
                          size: 14, action: PlayerScript.toggleShuffle)
            Spacer()
            controlButton(systemName: "backward.fill", size: 18, action: PlayerScript.prev)
            controlButton(
                systemName: store.snapshot.isPlaying ? "pause.fill" : "play.fill",
                size: 24, action: { PlayerScript.playPause() })
                .padding(.horizontal, 10)
            controlButton(systemName: "forward.fill", size: 18, action: PlayerScript.next)
            Spacer()
            controlButton(
                systemName: repeatIcon,
                size: 14,
                action: PlayerScript.cycleRepeat)
        }
    }

    var repeatIcon: String {
        switch store.snapshot.repeatMode {
        case "one": return "repeat.1"
        case "all": return "repeat.circle.fill"
        default:    return "repeat"
        }
    }

    func controlButton(systemName: String, size: CGFloat, action: @escaping () -> Void) -> some View {
        Button(action: {
            action()
            // Refresh quickly so UI reflects new state without waiting for the next poll tick.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { store.refresh() }
        }) {
            Image(systemName: systemName)
                .font(.system(size: size, weight: .semibold))
                .frame(width: max(28, size + 8), height: max(28, size + 8))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    var footer: some View {
        HStack(spacing: 8) {
            Image(systemName: "speaker.wave.2.fill").foregroundStyle(.secondary)
            Text(store.deviceLabel.isEmpty ? "No output device" : store.deviceLabel)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Spacer()
            Button(action: onActions) {
                Image(systemName: "ellipsis.circle")
                    .font(.system(size: 16))
            }
            .buttonStyle(.plain)
            .help("More actions")
        }
    }

    func formatTime(_ t: Double) -> String {
        let s = Int(t.rounded())
        return String(format: "%d:%02d", s / 60, s % 60)
    }
}

// MARK: - App delegate -

// Borderless floating panel that hosts the SwiftUI now-playing card.
// NSPopover always renders an arrow + standard padding gap; NSPanel lets us
// snap the card flush below the status item like macOS's own Now Playing
// widget.
final class NowPlayingPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    var statusItem: NSStatusItem!
    var panel: NowPlayingPanel!
    let store = PlayerStore()
    let watcher = LogWatcher()
    var actionMenu: NSMenu!
    var eventMonitor: Any?
    private var cancellables: Set<AnyCancellable> = []

    func applicationDidFinishLaunching(_ notification: Notification) {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let btn = statusItem.button {
            btn.image = NSImage(
                systemSymbolName: "music.note", accessibilityDescription: "Audio Format")
            btn.image?.isTemplate = true
            btn.action = #selector(handleClick(_:))
            btn.target = self
            // Status item buttons only forward left-click by default; opt in
            // to right-click so handleClick can route to the actions menu.
            btn.sendAction(on: [.leftMouseUp, .rightMouseUp])
        }

        // Borderless flyout panel hosting the SwiftUI now-playing card.
        // becomesKeyOnlyIfNeeded keeps Music.app keyboard focus while still
        // letting the panel's transport buttons receive clicks.
        let card = NowPlayingCard(store: store) { [weak self] in
            self?.showActionMenu()
        }
        let host = NSHostingView(rootView: card)
        host.translatesAutoresizingMaskIntoConstraints = false
        panel = NowPlayingPanel(
            contentRect: NSRect(x: 0, y: 0, width: 360, height: 220),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false)
        panel.isFloatingPanel = true
        panel.becomesKeyOnlyIfNeeded = true
        panel.level = .popUpMenu
        panel.hidesOnDeactivate = false
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]

        // Wrap the SwiftUI host in a vibrancy blur so the popover gets the
        // standard macOS menu material (translucent rounded card) instead
        // of floating over the desktop with no background.
        let blur = NSVisualEffectView()
        blur.material = .menu
        blur.blendingMode = .behindWindow
        blur.state = .active
        blur.wantsLayer = true
        blur.layer?.cornerRadius = 12
        blur.layer?.masksToBounds = true
        blur.layer?.borderWidth = 0.5
        blur.layer?.borderColor = NSColor.separatorColor.withAlphaComponent(0.4).cgColor
        blur.addSubview(host)
        NSLayoutConstraint.activate([
            host.leadingAnchor.constraint(equalTo: blur.leadingAnchor),
            host.trailingAnchor.constraint(equalTo: blur.trailingAnchor),
            host.topAnchor.constraint(equalTo: blur.topAnchor),
            host.bottomAnchor.constraint(equalTo: blur.bottomAnchor),
        ])
        panel.contentView = blur

        actionMenu = NSMenu()
        actionMenu.delegate = self

        watchMenubarFlag()
        watchMusicAppLifecycle()
        applyMusicState()
        applyDisplayMode()

        // Re-render the status item title whenever the active format
        // changes so format-mode users see the live sample rate.
        store.$format
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in self?.applyDisplayMode() }
            .store(in: &cancellables)

        watcher.onChange = { [weak self] in
            DispatchQueue.main.async { self?.store.refresh() }
        }
        watcher.start()
        store.refresh()
    }

    // MARK: - Music.app lifecycle gating -

    private func musicIsRunning() -> Bool {
        NSWorkspace.shared.runningApplications.contains {
            $0.bundleIdentifier == "com.apple.Music"
        }
    }

    func watchMusicAppLifecycle() {
        let nc = NSWorkspace.shared.notificationCenter
        nc.addObserver(
            self, selector: #selector(musicAppChanged(_:)),
            name: NSWorkspace.didLaunchApplicationNotification, object: nil)
        nc.addObserver(
            self, selector: #selector(musicAppChanged(_:)),
            name: NSWorkspace.didTerminateApplicationNotification, object: nil)
    }

    @objc func musicAppChanged(_ note: Notification) {
        guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey]
                as? NSRunningApplication,
              app.bundleIdentifier == "com.apple.Music"
        else { return }
        applyMusicState()
    }

    /// Visibility + activity gating. Hides the menubar icon and stops the
    /// log watcher when Music.app isn't running (saves CPU + visual
    /// clutter); resumes both when Music.app launches again. Manual
    /// `menubar.off` flag still wins.
    func applyMusicState() {
        let musicUp = musicIsRunning()
        let manuallyHidden = FileManager.default.fileExists(atPath: menubarOffPath)
        statusItem.isVisible = musicUp && !manuallyHidden

        if musicUp {
            if !watcher.isRunning { watcher.start() }
        } else {
            if watcher.isRunning { watcher.stop() }
            if panel.isVisible { dismissPanel() }
        }
    }

    @objc func handleClick(_ sender: NSStatusBarButton) {
        let evt = NSApp.currentEvent
        if evt?.type == .rightMouseUp {
            showActionMenu()
        } else {
            togglePopover(sender)
        }
    }

    func togglePopover(_ sender: NSStatusBarButton) {
        if panel.isVisible {
            dismissPanel()
            return
        }
        // Re-size panel to current SwiftUI content height. The contentView
        // is the blur wrapper (NSVisualEffectView has no intrinsic size), so
        // we read fittingSize from the SwiftUI host inside it instead.
        let host = panel.contentView?.subviews.first
        let fit = host?.fittingSize ?? .zero
        if fit.width > 50 && fit.height > 50 {
            panel.setContentSize(fit)
        } else {
            panel.setContentSize(NSSize(width: 360, height: 220))
        }
        let buttonFrame = sender.window?.convertToScreen(sender.frame) ?? .zero
        let panelSize = panel.frame.size
        let x = buttonFrame.midX - panelSize.width / 2
        let y = buttonFrame.minY - panelSize.height - 4
        panel.setFrameOrigin(NSPoint(x: x, y: y))
        panel.orderFrontRegardless()
        store.startPolling()

        // Dismiss when clicking anywhere else.
        if eventMonitor == nil {
            eventMonitor = NSEvent.addGlobalMonitorForEvents(
                matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
                self?.dismissPanel()
            }
        }
    }

    func dismissPanel() {
        panel.orderOut(nil)
        store.stopPolling()
        if let m = eventMonitor { NSEvent.removeMonitor(m); eventMonitor = nil }
    }

    func showActionMenu() {
        // Dismiss the now-playing panel first so the action menu doesn't
        // overlap on top of it.
        if panel.isVisible { dismissPanel() }
        rebuildActionMenu()
        statusItem.menu = actionMenu
        statusItem.button?.performClick(nil)
        // Re-clear menu so the next left-click goes back to the panel.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
            self?.statusItem.menu = nil
        }
    }

    private var dirSource: DispatchSourceFileSystemObject?
    private var dirFD: Int32 = -1
    func watchMenubarFlag() {
        let fd = open(supportDir, O_EVTONLY)
        guard fd >= 0 else { return }
        dirFD = fd
        let src = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd, eventMask: [.write, .delete, .rename],
            queue: DispatchQueue.main)
        src.setEventHandler { [weak self] in
            self?.applyMusicState()
            self?.applyDisplayMode()
        }
        src.setCancelHandler { [weak self] in
            if let fd = self?.dirFD, fd >= 0 { close(fd) }
        }
        dirSource = src
        src.resume()
    }

    func rebuildActionMenu() {
        actionMenu.removeAllItems()

        // Switch format submenu.
        if let device = defaultOutputDevice() {
            let header = NSMenuItem(title: "Output: \(deviceName(device))", action: nil, keyEquivalent: "")
            header.isEnabled = false
            actionMenu.addItem(header)

            let switchItem = NSMenuItem(title: "Switch Format", action: nil, keyEquivalent: "")
            let switchMenu = NSMenu()
            let streams = outputStreams(device)
            let current = streams.first.flatMap(currentFormat).map(formatKey)
            var seen = Set<DeviceFormat>(); var fmts: [DeviceFormat] = []
            for s in streams {
                for f in availableFormats(s) {
                    let k = formatKey(f); if seen.insert(k).inserted { fmts.append(k) }
                }
            }
            fmts.sort { $0.rate != $1.rate ? $0.rate < $1.rate
                : ($0.bits != $1.bits ? $0.bits < $1.bits : (!$0.isFloat && $1.isFloat)) }
            for f in fmts {
                let mi = NSMenuItem(title: f.label, action: #selector(applyFormatItem(_:)), keyEquivalent: "")
                mi.target = self
                mi.representedObject = f
                if current == f { mi.state = .on }
                switchMenu.addItem(mi)
            }
            switchItem.submenu = switchMenu
            actionMenu.addItem(switchItem)
            actionMenu.addItem(.separator())
        }

        let autoOn = !FileManager.default.fileExists(atPath: offSwitchPath)
        let toggle = NSMenuItem(
            title: autoOn ? "Disable Auto-Follow Song Format" : "Enable Auto-Follow Song Format",
            action: #selector(toggleAutoApply), keyEquivalent: "")
        toggle.target = self
        actionMenu.addItem(toggle)

        let showRate = NSMenuItem(
            title: "Show Sample Rate in Menubar",
            action: #selector(toggleDisplayMode), keyEquivalent: "")
        showRate.target = self
        showRate.state = (currentDisplayMode() == "format") ? .on : .off
        actionMenu.addItem(showRate)
        actionMenu.addItem(.separator())

        if NSWorkspace.shared.urlForApplication(
            withBundleIdentifier: "com.runningwithcrayons.Alfred") != nil
        {
            let af = NSMenu()
            for kw in ["np", "audio", "npauto", "nplog", "npmenu"] {
                let mi = NSMenuItem(title: kw, action: #selector(openAlfred(_:)), keyEquivalent: "")
                mi.target = self
                mi.representedObject = kw
                af.addItem(mi)
            }
            let alf = NSMenuItem(title: "Open in Alfred", action: nil, keyEquivalent: "")
            alf.submenu = af
            actionMenu.addItem(alf)
            actionMenu.addItem(.separator())
        }

        let hide = NSMenuItem(title: "Hide Menubar Icon", action: #selector(hideMenubar), keyEquivalent: "")
        hide.target = self
        hide.toolTip = "Daemon keeps running. Use Alfred `npmenu` to show again."
        actionMenu.addItem(hide)

        let q = NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q")
        q.target = self
        actionMenu.addItem(q)
    }

    func menuDidClose(_ menu: NSMenu) {
        statusItem.menu = nil // restore popover behavior
    }

    @objc func applyFormatItem(_ sender: NSMenuItem) {
        guard let f = sender.representedObject as? DeviceFormat else { return }
        _ = applyAudioFormat(rate: f.rate, bits: f.bits)
    }

    @objc func toggleAutoApply() {
        try? FileManager.default.createDirectory(
            atPath: supportDir, withIntermediateDirectories: true)
        if FileManager.default.fileExists(atPath: offSwitchPath) {
            try? FileManager.default.removeItem(atPath: offSwitchPath)
        } else {
            FileManager.default.createFile(atPath: offSwitchPath, contents: nil)
        }
    }

    @objc func openAlfred(_ sender: NSMenuItem) {
        guard let kw = sender.representedObject as? String else { return }
        PlayerScript.openAlfred(kw)
    }

    @objc func hideMenubar() {
        try? FileManager.default.createDirectory(
            atPath: supportDir, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: menubarOffPath, contents: nil)
    }

    // MARK: - Status item display mode -

    /// "icon" (default) renders the music.note glyph; "format" replaces it
    /// with the live sample rate as text (e.g. "48 kHz"). When format mode
    /// is on but the rate is unknown, falls back to the icon so the
    /// menubar slot is never blank.
    private func currentDisplayMode() -> String {
        if let s = try? String(contentsOfFile: displayModePath, encoding: .utf8),
           s.trimmingCharacters(in: .whitespacesAndNewlines) == "format"
        { return "format" }
        return "icon"
    }

    private func rateLabel(_ rate: Int?) -> String? {
        guard let rate, rate > 0 else { return nil }
        let khz = Double(rate) / 1000.0
        return khz == khz.rounded()
            ? "\(Int(khz)) kHz"
            : String(format: "%.1f kHz", khz)
    }

    func applyDisplayMode() {
        guard let btn = statusItem.button else { return }
        if currentDisplayMode() == "format", let label = rateLabel(store.format.rate) {
            btn.image = nil
            btn.title = label
        } else {
            btn.title = ""
            let img = NSImage(systemSymbolName: "music.note", accessibilityDescription: "Audio Format")
            img?.isTemplate = true
            btn.image = img
        }
    }

    @objc func toggleDisplayMode() {
        try? FileManager.default.createDirectory(
            atPath: supportDir, withIntermediateDirectories: true)
        let next = currentDisplayMode() == "format" ? "icon" : "format"
        try? next.write(toFile: displayModePath, atomically: true, encoding: .utf8)
        applyDisplayMode()
    }

    @objc func quit() { NSApp.terminate(nil) }

}

// Activation policy is set via LSUIElement=true in AppBundle.Info.plist —
// no runtime setActivationPolicy call needed.
let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.run()
