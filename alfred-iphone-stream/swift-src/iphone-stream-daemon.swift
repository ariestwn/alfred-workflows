import AVFoundation
import Foundation

// MARK: - Exit codes
let EXIT_OK: Int32 = 0
let EXIT_NO_IPHONE: Int32 = 1
let EXIT_INPUT_FAILED: Int32 = 2
let EXIT_CANNOT_ADD_INPUT: Int32 = 3
let EXIT_CANNOT_ADD_OUTPUT: Int32 = 4
let EXIT_RUNTIME_ERROR: Int32 = 5
let EXIT_TCC_DENIED: Int32 = 10

// MARK: - Paths
let HOME = FileManager.default.homeDirectoryForCurrentUser
// Alfred has its own state directory; never control the Raycast daemon's PID.
func argument(_ name: String) -> String? {
    guard let index = CommandLine.arguments.firstIndex(of: name), index + 1 < CommandLine.arguments.count else { return nil }
    return CommandLine.arguments[index + 1]
}
let launchID = argument("--launch-id") ?? "manual"
let stateDir = (argument("--state-dir") ?? ProcessInfo.processInfo.environment["IPHONE_STREAM_STATE_DIR"]).map {
    URL(fileURLWithPath: $0, isDirectory: true)
} ?? HOME.appendingPathComponent("Library/Application Support/Alfred/Workflow Data/com.ariestwn.iphone-stream")
let stateFile = stateDir.appendingPathComponent("state.json")
let pidFile = stateDir.appendingPathComponent("daemon.pid")

// MARK: - Helpers
func ensureStateDir() {
    try? FileManager.default.createDirectory(at: stateDir, withIntermediateDirectories: true)
}

func writePidFile() {
    let pid = String(getpid())
    try? pid.write(to: pidFile, atomically: true, encoding: .utf8)
}

func clearPidFile() {
    try? FileManager.default.removeItem(at: pidFile)
    try? FileManager.default.removeItem(at: stateDir.appendingPathComponent("startup.pid"))
}

func reportStartup(_ phase: String, message: String = "", code: Int32 = 0) {
    let report: [String: Any] = ["launchID": launchID, "phase": phase, "message": message, "code": code, "pid": getpid()]
    if let data = try? JSONSerialization.data(withJSONObject: report) {
        try? data.write(to: stateDir.appendingPathComponent("startup.json"), options: .atomic)
    }
}

func failStartup(_ code: Int32) -> Never {
    let log = (try? String(contentsOf: stateDir.appendingPathComponent("daemon.log"), encoding: .utf8)) ?? ""
    reportStartup("failed", message: log.split(separator: "\n").last.map(String.init) ?? "Helper gagal memulai audio.", code: code)
    clearPidFile()
    exit(code)
}

func logErr(_ msg: String) {
    FileHandle.standardError.write((msg + "\n").data(using: .utf8) ?? Data())
}

func readVolume() -> Float {
    guard let data = try? Data(contentsOf: stateFile),
          let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
          let v = json["volume"] as? Double else {
        return 1.0
    }
    return Float(max(0.0, min(1.0, v)))
}

// MARK: - TCC check
func ensureMicrophonePermission() {
    let status = AVCaptureDevice.authorizationStatus(for: .audio)
    switch status {
    case .authorized:
        return
    case .notDetermined:
        let sema = DispatchSemaphore(value: 0)
        var granted = false
        AVCaptureDevice.requestAccess(for: .audio) { ok in
            granted = ok
            sema.signal()
        }
        _ = sema.wait(timeout: .now() + .seconds(25))
        if !granted {
            logErr("Microphone permission not granted. Enable in System Settings → Privacy & Security → Microphone.")
            failStartup(EXIT_TCC_DENIED)
        }
    case .denied, .restricted:
        logErr("Microphone permission denied or restricted. Enable in System Settings → Privacy & Security → Microphone.")
        failStartup(EXIT_TCC_DENIED)
    @unknown default:
        logErr("Unknown microphone authorization status: \(status.rawValue).")
        failStartup(EXIT_TCC_DENIED)
    }
}

// MARK: - iPhone discovery
let modelIDPattern = try! NSRegularExpression(
    pattern: "^iPhone:[0-9A-Fa-f]{4}:[0-9A-Fa-f]{4}$",
    options: []
)

func isCoreAudioIPhone(_ d: AVCaptureDevice) -> Bool {
    let range = NSRange(location: 0, length: d.modelID.utf16.count)
    return modelIDPattern.firstMatch(in: d.modelID, options: [], range: range) != nil
}

// Only the CoreAudio path (Inter-device Audio and MIDI Mode device) is supported.
// The AVCapture "iPhone Mic" path triggers iOS recording indicator and is intentionally
// excluded — users must enable Audio MIDI Mode in Audio MIDI Setup.
func findIPhone() -> AVCaptureDevice? {
    let discovery = AVCaptureDevice.DiscoverySession(
        deviceTypes: [.microphone],
        mediaType: .audio,
        position: .unspecified
    )
    return discovery.devices.first(where: isCoreAudioIPhone)
}

// MARK: - File watcher
final class StateWatcher {
    private var source: DispatchSourceFileSystemObject?
    private var fd: Int32 = -1
    private let onChange: () -> Void

    init(path: String, onChange: @escaping () -> Void) {
        self.onChange = onChange
        start(path: path)
    }

    private func start(path: String) {
        fd = open(path, O_EVTONLY)
        guard fd >= 0 else {
            logErr("StateWatcher: failed to open \(path) (errno=\(errno))")
            return
        }
        let s = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd,
            eventMask: [.write, .extend, .delete, .rename],
            queue: .main
        )
        s.setEventHandler { [weak self] in
            guard let self = self else { return }
            let mask = s.data
            self.onChange()
            if mask.contains(.delete) || mask.contains(.rename) {
                self.stop()
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) { [weak self] in
                    self?.start(path: path)
                }
            }
        }
        s.setCancelHandler { [fd = self.fd] in
            if fd >= 0 { close(fd) }
        }
        s.resume()
        source = s
    }

    func stop() {
        source?.cancel()
        source = nil
        fd = -1
    }
}

// MARK: - Main
ensureStateDir()
// LaunchServices doesn't inherit the Rust controller's redirected stderr.
let logPath = stateDir.appendingPathComponent("daemon.log").path
freopen(logPath, "a", stderr)
freopen(logPath, "a", stdout)
try? String(getpid()).write(to: stateDir.appendingPathComponent("startup.pid"), atomically: true, encoding: .utf8)
reportStartup("starting")
logErr("Starting iPhone Stream Helper (bundle=\(Bundle.main.bundleIdentifier ?? "unknown"), pid=\(getpid()))")
if CommandLine.arguments.contains("--diagnose") {
    reportStartup("diagnosed", message: "Microphone authorization status: \(AVCaptureDevice.authorizationStatus(for: .audio).rawValue)")
    clearPidFile()
    exit(EXIT_OK)
}
ensureMicrophonePermission()

if !FileManager.default.fileExists(atPath: stateFile.path) {
    let initial = Data(#"{"volume":1.0}"#.utf8)
    try? initial.write(to: stateFile)
}

guard let iphone = findIPhone() else {
    logErr("No iPhone audio device found. Connect via USB and enable 'Inter-device Audio and MIDI Mode' in Audio MIDI Setup.app.")
    failStartup(EXIT_NO_IPHONE)
}

let session = AVCaptureSession()
session.beginConfiguration()

let input: AVCaptureDeviceInput
do {
    input = try AVCaptureDeviceInput(device: iphone)
} catch {
    logErr("Failed to create input: \(error)")
    failStartup(EXIT_INPUT_FAILED)
}
guard session.canAddInput(input) else {
    logErr("Cannot add iPhone input to session.")
    failStartup(EXIT_CANNOT_ADD_INPUT)
}
session.addInput(input)

let preview = AVCaptureAudioPreviewOutput()
preview.outputDeviceUniqueID = nil
preview.volume = readVolume()
guard session.canAddOutput(preview) else {
    logErr("Cannot add audio preview output.")
    failStartup(EXIT_CANNOT_ADD_OUTPUT)
}
session.addOutput(preview)

session.commitConfiguration()
session.startRunning()
guard session.isRunning else {
    logErr("Audio session failed to start. Check Microphone permission and Audio MIDI Mode.")
    failStartup(EXIT_RUNTIME_ERROR)
}

let watcher = StateWatcher(path: stateFile.path) {
    let v = readVolume()
    preview.volume = v
    logErr("Volume updated → \(Int(v * 100))%")
}
_ = watcher

// MARK: - Cleanup
func shutdown(reason: String, exitCode: Int32) {
    logErr("Shutting down: \(reason)")
    if session.isRunning { session.stopRunning() }
    watcher.stop()
    clearPidFile()
    exit(exitCode)
}

// MARK: - Signal handling (DispatchSource lets us call non-async-signal-safe code)
// Ignore default disposition BEFORE creating the dispatch source, otherwise the
// kernel may apply the default action (terminate) before the handler runs.
signal(SIGTERM, SIG_IGN)
let sigtermSrc = DispatchSource.makeSignalSource(signal: SIGTERM, queue: .main)
sigtermSrc.setEventHandler { shutdown(reason: "SIGTERM", exitCode: EXIT_OK) }
sigtermSrc.resume()

signal(SIGINT, SIG_IGN)
let sigintSrc = DispatchSource.makeSignalSource(signal: SIGINT, queue: .main)
sigintSrc.setEventHandler { shutdown(reason: "SIGINT", exitCode: EXIT_OK) }
sigintSrc.resume()

// MARK: - iPhone disconnect / session error observers
let nc = NotificationCenter.default

nc.addObserver(forName: AVCaptureDevice.wasDisconnectedNotification, object: nil, queue: .main) { note in
    guard let device = note.object as? AVCaptureDevice, device.uniqueID == iphone.uniqueID else { return }
    shutdown(reason: "iPhone disconnected", exitCode: EXIT_OK)
}

nc.addObserver(forName: AVCaptureSession.runtimeErrorNotification, object: session, queue: .main) { note in
    let err = note.userInfo?[AVCaptureSessionErrorKey] as? Error
    shutdown(reason: "session runtime error: \(err?.localizedDescription ?? "unknown")", exitCode: EXIT_RUNTIME_ERROR)
}

logErr("iPhone Stream daemon running (pid=\(getpid()), device=\(iphone.localizedName), modelID=\(iphone.modelID), volume=\(Int(preview.volume * 100))%)")
// Publish readiness only after signal handlers and disconnect observers exist.
writePidFile()
reportStartup("running")

RunLoop.main.run()
