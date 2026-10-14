import CoreFoundation
import Foundation

// Helper for toggling Inter-Device Audio and MIDI (IDAM) mode on a paired iPhone
// by talking to the iOS `com.apple.idamd` lockdown service via the private
// MobileDevice.framework — the same path Apple's Audio MIDI Setup uses.
//
// Usage:
//   idam-helper get
//   idam-helper set true|false
//
// stdout: single JSON line on success.
// stderr: human-readable error on failure.

// MARK: - Exit codes
let EXIT_OK: Int32 = 0
let EXIT_NO_DEVICE: Int32 = 1
let EXIT_USAGE: Int32 = 2
let EXIT_CONNECT_FAILED: Int32 = 3
let EXIT_NOT_PAIRED: Int32 = 4
let EXIT_VALIDATE_FAILED: Int32 = 5
let EXIT_SESSION_FAILED: Int32 = 6
let EXIT_SERVICE_FAILED: Int32 = 7
let EXIT_IO_FAILED: Int32 = 8
let EXIT_FRAMEWORK_FAILED: Int32 = 20

// MARK: - MobileDevice.framework dynamic binding
let MD_PATH = "/System/Library/PrivateFrameworks/MobileDevice.framework/MobileDevice"
guard let mdHandle = dlopen(MD_PATH, RTLD_LAZY) else {
    FileHandle.standardError.write(Data("error: cannot load MobileDevice.framework\n".utf8))
    exit(EXIT_FRAMEWORK_FAILED)
}

func sym(_ name: String) -> UnsafeMutableRawPointer {
    guard let p = dlsym(mdHandle, name) else {
        FileHandle.standardError.write(Data("error: missing symbol \(name)\n".utf8))
        exit(EXIT_FRAMEWORK_FAILED)
    }
    return p
}

typealias AMDeviceRef = OpaquePointer
typealias AMDServiceConnectionRef = OpaquePointer
typealias AMDeviceNotificationRef = OpaquePointer

// C layout of `am_device_notification_callback_info`:
//   { am_device *dev; unsigned int msg; }
// Read as raw pointer to stay @convention(c) compatible.
let CB_INFO_DEV_OFFSET = 0
let CB_INFO_MSG_OFFSET = MemoryLayout<AMDeviceRef?>.size

typealias DeviceCallback = @convention(c) (
    UnsafeMutableRawPointer?,
    UnsafeMutableRawPointer?
) -> Void

typealias NotificationSubscribeFn = @convention(c) (
    DeviceCallback, UInt32, UInt32, UnsafeMutableRawPointer?,
    UnsafeMutablePointer<AMDeviceNotificationRef?>?
) -> Int32
typealias DeviceFn = @convention(c) (AMDeviceRef?) -> Int32
typealias CopyIdFn = @convention(c) (AMDeviceRef?) -> Unmanaged<CFString>?
typealias SecureStartFn = @convention(c) (
    AMDeviceRef?, CFString, CFDictionary?,
    UnsafeMutablePointer<AMDServiceConnectionRef?>?
) -> Int32
typealias SendMsgFn = @convention(c) (
    AMDServiceConnectionRef, CFPropertyList, CFPropertyListFormat
) -> Int32
typealias RecvMsgFn = @convention(c) (
    AMDServiceConnectionRef,
    UnsafeMutablePointer<Unmanaged<CFPropertyList>?>?,
    UnsafeMutablePointer<CFPropertyListFormat>?
) -> Int32
typealias InvalidateFn = @convention(c) (AMDServiceConnectionRef) -> Void

let AMDeviceNotificationSubscribe = unsafeBitCast(
    sym("AMDeviceNotificationSubscribe"), to: NotificationSubscribeFn.self)
let AMDeviceConnect = unsafeBitCast(sym("AMDeviceConnect"), to: DeviceFn.self)
let AMDeviceDisconnect = unsafeBitCast(sym("AMDeviceDisconnect"), to: DeviceFn.self)
let AMDeviceIsPaired = unsafeBitCast(sym("AMDeviceIsPaired"), to: DeviceFn.self)
let AMDeviceValidatePairing = unsafeBitCast(sym("AMDeviceValidatePairing"), to: DeviceFn.self)
let AMDeviceStartSession = unsafeBitCast(sym("AMDeviceStartSession"), to: DeviceFn.self)
let AMDeviceStopSession = unsafeBitCast(sym("AMDeviceStopSession"), to: DeviceFn.self)
let AMDeviceCopyDeviceIdentifier = unsafeBitCast(
    sym("AMDeviceCopyDeviceIdentifier"), to: CopyIdFn.self)
let AMDeviceSecureStartService = unsafeBitCast(
    sym("AMDeviceSecureStartService"), to: SecureStartFn.self)
let AMDServiceConnectionSendMessage = unsafeBitCast(
    sym("AMDServiceConnectionSendMessage"), to: SendMsgFn.self)
let AMDServiceConnectionReceiveMessage = unsafeBitCast(
    sym("AMDServiceConnectionReceiveMessage"), to: RecvMsgFn.self)
let AMDServiceConnectionInvalidate = unsafeBitCast(
    sym("AMDServiceConnectionInvalidate"), to: InvalidateFn.self)

// MARK: - Helpers
func logErr(_ msg: String) {
    FileHandle.standardError.write(Data((msg + "\n").utf8))
}

func hex(_ code: Int32) -> String { String(format: "0x%x", UInt32(bitPattern: code)) }

enum Action {
    case get
    case set(Bool)
}

// MARK: - Parse args
let args = CommandLine.arguments
let action: Action = {
    guard args.count >= 2 else {
        logErr("usage: idam-helper get | set <true|false>")
        exit(EXIT_USAGE)
    }
    switch args[1] {
    case "get":
        return .get
    case "set":
        guard args.count == 3, let v = Bool(args[2]) else {
            logErr("usage: idam-helper set <true|false>")
            exit(EXIT_USAGE)
        }
        return .set(v)
    default:
        logErr("usage: idam-helper get | set <true|false>")
        exit(EXIT_USAGE)
    }
}()

// MARK: - Wait for first device (timeout 5s)
let DEVICE_TIMEOUT: CFTimeInterval = 5.0

class State {
    var device: AMDeviceRef?
    var udid: String = ""
}
let state = State()
let statePtr = Unmanaged.passUnretained(state).toOpaque()

let callback: DeviceCallback = { infoPtr, ctx in
    guard let info = infoPtr, let ctx = ctx else { return }
    let msg = info.load(fromByteOffset: CB_INFO_MSG_OFFSET, as: UInt32.self)
    guard msg == 1 else { return }  // ADNCI_MSG_CONNECTED
    guard let dev = info.load(fromByteOffset: CB_INFO_DEV_OFFSET, as: AMDeviceRef?.self)
    else { return }
    let s = Unmanaged<State>.fromOpaque(ctx).takeUnretainedValue()
    if s.device != nil { return }
    s.device = dev
    if let cf = AMDeviceCopyDeviceIdentifier(dev) {
        s.udid = cf.takeRetainedValue() as String
    }
    CFRunLoopStop(CFRunLoopGetCurrent())
}

var notification: AMDeviceNotificationRef?
let subStatus = AMDeviceNotificationSubscribe(callback, 0, 0, statePtr, &notification)
if subStatus != 0 {
    logErr("AMDeviceNotificationSubscribe failed: \(hex(subStatus))")
    exit(EXIT_FRAMEWORK_FAILED)
}

CFRunLoopRunInMode(.defaultMode, DEVICE_TIMEOUT, false)

guard let device = state.device else {
    logErr("no iOS device attached")
    exit(EXIT_NO_DEVICE)
}

// MARK: - Connect / pair / session
let connectStatus = AMDeviceConnect(device)
if connectStatus != 0 {
    logErr("AMDeviceConnect failed: \(hex(connectStatus))")
    exit(EXIT_CONNECT_FAILED)
}
defer { _ = AMDeviceDisconnect(device) }

if AMDeviceIsPaired(device) == 0 {
    logErr("device not paired with this Mac")
    exit(EXIT_NOT_PAIRED)
}

let validateStatus = AMDeviceValidatePairing(device)
if validateStatus != 0 {
    logErr("AMDeviceValidatePairing failed: \(hex(validateStatus))")
    exit(EXIT_VALIDATE_FAILED)
}

let sessionStatus = AMDeviceStartSession(device)
if sessionStatus != 0 {
    logErr("AMDeviceStartSession failed: \(hex(sessionStatus))")
    exit(EXIT_SESSION_FAILED)
}
defer { _ = AMDeviceStopSession(device) }

// MARK: - Start com.apple.idamd
var conn: AMDServiceConnectionRef?
let serviceName = "com.apple.idamd" as CFString
let serviceStatus = AMDeviceSecureStartService(device, serviceName, nil, &conn)
guard serviceStatus == 0, let connection = conn else {
    logErr("AMDeviceSecureStartService(com.apple.idamd) failed: \(hex(serviceStatus))")
    exit(EXIT_SERVICE_FAILED)
}
defer { AMDServiceConnectionInvalidate(connection) }

// MARK: - Talk IDAM protocol
func send(_ dict: [String: Any]) -> Int32 {
    let cf = dict as CFDictionary
    return AMDServiceConnectionSendMessage(connection, cf, .xmlFormat_v1_0)
}

func recv() -> [String: Any]? {
    var raw: Unmanaged<CFPropertyList>?
    var fmt: CFPropertyListFormat = .xmlFormat_v1_0
    let status = AMDServiceConnectionReceiveMessage(connection, &raw, &fmt)
    if status != 0 { return nil }
    return raw?.takeRetainedValue() as? [String: Any]
}

func emitJSON(_ obj: [String: Any]) {
    if let data = try? JSONSerialization.data(withJSONObject: obj, options: []) {
        FileHandle.standardOutput.write(data)
        FileHandle.standardOutput.write(Data("\n".utf8))
    }
}

switch action {
case .get:
    let sendErr = send(["Configuration Inquiry": true])
    if sendErr != 0 {
        logErr("send Configuration Inquiry failed: \(hex(sendErr))")
        exit(EXIT_IO_FAILED)
    }
    guard let reply = recv() else {
        logErr("recv Configuration Inquiry failed")
        exit(EXIT_IO_FAILED)
    }
    let active = (reply["IDAM Configuration Active"] as? Bool) ?? false
    let lowEnd = (reply["Low End Hardware"] as? Bool) ?? false
    emitJSON([
        "ok": true,
        "udid": state.udid,
        "enabled": active,
        "lowEndHardware": lowEnd,
    ])

case .set(let value):
    // The iOS daemon sends no response unless "Configuration Inquiry" is also
    // present, and on `true` the device re-enumerates which drops the
    // connection. Send-and-forget is correct here.
    let sendErr = send(["Set IDAM Configuration": value])
    if sendErr != 0 {
        logErr("send Set IDAM Configuration failed: \(hex(sendErr))")
        exit(EXIT_IO_FAILED)
    }
    emitJSON([
        "ok": true,
        "udid": state.udid,
        "enabled": value,
    ])
}
