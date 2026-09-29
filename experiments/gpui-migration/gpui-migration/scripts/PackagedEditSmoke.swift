import AppKit
import ApplicationServices
import CoreGraphics
import Foundation
import PDFKit

struct SmokeFailure: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}

func require(_ condition: @autoclosure () -> Bool, _ message: String) throws {
    if !condition() { throw SmokeFailure(message) }
}

func attribute(_ element: AXUIElement, _ name: CFString) -> CFTypeRef? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, name, &value) == .success else { return nil }
    return value
}

func text(_ element: AXUIElement, _ name: CFString) -> String? {
    attribute(element, name) as? String
}

func children(_ element: AXUIElement) -> [AXUIElement] {
    (attribute(element, kAXChildrenAttribute as CFString) as? [AXUIElement]) ?? []
}

func descendants(_ root: AXUIElement) -> [AXUIElement] {
    var result: [AXUIElement] = []
    var pending = [root]
    while let current = pending.popLast(), result.count < 20_000 {
        result.append(current)
        pending.append(contentsOf: children(current))
    }
    return result
}

func pointValue(_ element: AXUIElement, _ name: CFString) -> CGPoint? {
    guard let raw = attribute(element, name) else { return nil }
    guard CFGetTypeID(raw) == AXValueGetTypeID() else { return nil }
    var point = CGPoint.zero
    guard AXValueGetValue(raw as! AXValue, .cgPoint, &point) else { return nil }
    return point
}

func sizeValue(_ element: AXUIElement, _ name: CFString) -> CGSize? {
    guard let raw = attribute(element, name) else { return nil }
    guard CFGetTypeID(raw) == AXValueGetTypeID() else { return nil }
    var size = CGSize.zero
    guard AXValueGetValue(raw as! AXValue, .cgSize, &size) else { return nil }
    return size
}

func bounds(_ element: AXUIElement) -> CGRect? {
    guard let origin = pointValue(element, kAXPositionAttribute as CFString),
          let size = sizeValue(element, kAXSizeAttribute as CFString),
          size.width.isFinite, size.height.isFinite, size.width > 0, size.height > 0 else { return nil }
    return CGRect(origin: origin, size: size)
}

func appElement(_ pid: pid_t) -> AXUIElement { AXUIElementCreateApplication(pid) }

func requireTrustedAX() throws {
    try require(AXIsProcessTrusted(), "macOS Accessibility trust is unavailable to the packaged edit driver")
}

func press(_ element: AXUIElement) throws {
    try require(AXUIElementPerformAction(element, kAXPressAction as CFString) == .success, "could not activate an accessible control")
}

func postKey(_ key: CGKeyCode, flags: CGEventFlags = []) throws {
    guard let down = CGEvent(keyboardEventSource: nil, virtualKey: key, keyDown: true),
          let up = CGEvent(keyboardEventSource: nil, virtualKey: key, keyDown: false) else {
        throw SmokeFailure("could not create keyboard event")
    }
    down.flags = flags
    up.flags = flags
    down.post(tap: .cghidEventTap)
    up.post(tap: .cghidEventTap)
}

func postMouse(_ type: CGEventType, at point: CGPoint) throws {
    let button: CGMouseButton = .left
    guard let event = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: point, mouseButton: button) else {
        throw SmokeFailure("could not create pointer event")
    }
    event.post(tap: .cghidEventTap)
}

func wait(_ seconds: TimeInterval = 0.2) { Thread.sleep(forTimeInterval: seconds) }

func applicationWindows(_ app: AXUIElement) -> [AXUIElement] {
    (attribute(app, kAXWindowsAttribute as CFString) as? [AXUIElement]) ?? []
}

func describe(_ element: AXUIElement) -> String {
    [kAXTitleAttribute, kAXDescriptionAttribute, kAXHelpAttribute]
        .compactMap { text(element, $0 as CFString) }
        .joined(separator: " | ")
}

func matching(_ app: AXUIElement, _ label: String) -> [AXUIElement] {
    descendants(app).filter { describe($0).split(separator: "|").contains { $0.trimmingCharacters(in: .whitespacesAndNewlines).localizedCaseInsensitiveCompare(label) == .orderedSame } }
}

func waitForSingleButton(_ app: AXUIElement, _ label: String, timeout: TimeInterval = 10) throws -> AXUIElement {
    let deadline = Date().addingTimeInterval(timeout)
    repeat {
        let buttons = matching(app, label).filter { text($0, kAXRoleAttribute as CFString) == kAXButtonRole as String }
        if buttons.count == 1 { return buttons[0] }
        wait(0.2)
    } while Date() < deadline
    let labels = descendants(app)
        .filter { text($0, kAXRoleAttribute as CFString) == kAXButtonRole as String }
        .map(describe)
        .filter { !$0.isEmpty }
        .prefix(100)
    throw SmokeFailure("expected exactly one accessible \(label) button; available buttons: \(Array(labels))")
}

func singleButtonIfPublished(_ app: AXUIElement, _ label: String, timeout: TimeInterval = 2) -> AXUIElement? {
    let deadline = Date().addingTimeInterval(timeout)
    repeat {
        let buttons = matching(app, label).filter { text($0, kAXRoleAttribute as CFString) == kAXButtonRole as String }
        if buttons.count == 1 { return buttons[0] }
        wait(0.2)
    } while Date() < deadline
    return nil
}

func activate(_ pid: pid_t) throws {
    guard let running = NSRunningApplication(processIdentifier: pid) else { throw SmokeFailure("packaged app PID is not running") }
    _ = running.activate(options: [.activateAllWindows])
    wait(0.5)
}

func performEdit(pid: pid_t) throws -> [String: Any] {
    try requireTrustedAX()
    try activate(pid)
    let app = appElement(pid)
    let windows = applicationWindows(app)
    try require(!windows.isEmpty, "packaged app has no accessible window")
    let rectangleButton = singleButtonIfPublished(app, "Rectangle")
    let rectangleActivation: String
    if let rectangleButton {
        try press(rectangleButton)
        rectangleActivation = "accessible-button: \(describe(rectangleButton))"
    } else {
        try postKey(0x0F) // R — the ordinary documented Rectangle shortcut.
        rectangleActivation = "keyboard-shortcut-r"
    }
    wait()

    let windowFrames = windows.compactMap(bounds)
    try require(windowFrames.count == windows.count, "could not read accessible window bounds")
    let frame = windowFrames.max { $0.width * $0.height < $1.width * $1.height }!
    try require(frame.width >= 900 && frame.height >= 600, "app window is smaller than the supported native smoke coordinate contract")
    // GPUI exposes toolbar buttons to AX, but its PDF paint surface has no AX child.
    // Anchor the drag to the visible primary-window frame and the fixed 1200x800
    // launch geometry; keep it in the central document viewport, clear of rails.
    let start = CGPoint(x: frame.minX + frame.width * 0.40, y: frame.minY + frame.height * 0.30)
    let end = CGPoint(x: frame.minX + frame.width * 0.58, y: frame.minY + frame.height * 0.58)
    try require(frame.insetBy(dx: 80, dy: 80).contains(start) && frame.insetBy(dx: 80, dy: 80).contains(end), "calculated rectangle drag is outside the supported window content area")
    let display = CGDisplayBounds(CGMainDisplayID())
    try require(display.contains(start) && display.contains(end), "calculated rectangle drag is outside the primary display")
    try postMouse(.mouseMoved, at: start)
    try postMouse(.leftMouseDown, at: start)
    for step in 1...12 {
        let fraction = CGFloat(step) / 12
        let point = CGPoint(x: start.x + (end.x - start.x) * fraction, y: start.y + (end.y - start.y) * fraction)
        try postMouse(.leftMouseDragged, at: point)
        wait(0.025)
    }
    try postMouse(.leftMouseUp, at: end)
    wait(0.6)

    let saveButton = try waitForSingleButton(app, "Save")
    try press(saveButton)
    wait(1.0)
    return ["rectangleActivation": rectangleActivation, "saveButton": describe(saveButton),
            "windowFrame": [frame.minX, frame.minY, frame.width, frame.height],
            "coordinateContract": "primary-window-relative central document viewport; GUI scale 100%; window minimum 900x600",
            "drag": [[start.x, start.y], [end.x, end.y]], "windowCount": windows.count]
}

func closeNormally(pid: pid_t) throws {
    try requireTrustedAX()
    try activate(pid)
    try postKey(0x0C, flags: .maskCommand) // Q
}

func verifyReopened(pid: pid_t, path: String) throws -> [String: Any] {
    try requireTrustedAX()
    try activate(pid)
    let app = appElement(pid)
    let windows = applicationWindows(app)
    try require(!windows.isEmpty, "reopened app has no accessible window")
    let title = windows.compactMap { text($0, kAXTitleAttribute as CFString) }.joined(separator: " | ")
    try require(title.localizedCaseInsensitiveContains(URL(fileURLWithPath: path).lastPathComponent), "reopened window title does not identify the edited PDF: \(title)")
    return ["windowCount": windows.count, "windowTitle": title]
}

func inspectPDF(path: String, expectedRectangles: Int? = 1) throws -> [String: Any] {
    guard let document = PDFDocument(url: URL(fileURLWithPath: path)), document.pageCount == 1,
          let page = document.page(at: 0) else { throw SmokeFailure("saved output is not a readable one-page PDF") }
    let annotations = page.annotations
    let rectangles = annotations.filter { $0.type == "Square" || $0.type == "Rect" }
    if let expectedRectangles {
        try require(rectangles.count == expectedRectangles, "expected \(expectedRectangles) rectangle annotations, found \(rectangles.count)")
    }
    let boxes = rectangles.map(\.bounds)
    for box in boxes {
        try require(box.width > 10 && box.height > 10 && box.width < 550 && box.height < 740,
                    "saved rectangle geometry is outside expected page bounds")
    }
    return ["pageCount": document.pageCount, "annotationCount": annotations.count, "rectangleCount": rectangles.count,
            "rectangleBounds": boxes.map { [$0.origin.x, $0.origin.y, $0.width, $0.height] }, "parser": "Apple PDFKit"]
}

func main() throws {
    let args = Array(CommandLine.arguments.dropFirst())
    guard args.count >= 1 else { throw SmokeFailure("usage: PackagedEditSmoke probe|edit PID|close PID|reopened PID PDF|inspect PDF") }
    let value: [String: Any]
    switch args[0] {
    case "probe":
        try requireTrustedAX()
        guard let event = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: CGPoint(x: 1, y: 1), mouseButton: .left) else {
            throw SmokeFailure("CoreGraphics pointer event creation is unavailable")
        }
        event.post(tap: .cghidEventTap)
        value = ["accessibilityTrusted": true, "cgEventPost": true]
    case "edit":
        guard args.count == 2, let pid = pid_t(args[1]) else { throw SmokeFailure("edit requires PID") }
        value = try performEdit(pid: pid)
    case "close":
        guard args.count == 2, let pid = pid_t(args[1]) else { throw SmokeFailure("close requires PID") }
        try closeNormally(pid: pid)
        value = ["normalQuitPosted": true]
    case "reopened":
        guard args.count == 3, let pid = pid_t(args[1]) else { throw SmokeFailure("reopened requires PID and PDF path") }
        value = try verifyReopened(pid: pid, path: args[2])
    case "inspect":
        guard args.count == 2 else { throw SmokeFailure("inspect requires PDF path") }
        value = try inspectPDF(path: args[1])
    case "inspect-baseline":
        guard args.count == 2 else { throw SmokeFailure("inspect-baseline requires PDF path") }
        value = try inspectPDF(path: args[1], expectedRectangles: nil)
    default:
        throw SmokeFailure("unknown helper command")
    }
    let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
    print(String(decoding: data, as: UTF8.self))
}

do {
    try main()
} catch {
    fputs("PackagedEditSmoke: \(error)\n", stderr)
    exit(1)
}
