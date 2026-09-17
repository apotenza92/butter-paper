// Native capture boundary. No microphone, image files, or network access.
// A single PNG is returned over stdout only after the user chooses Capture.
import AppKit
import AVFoundation
import CoreImage

final class CameraController: NSObject, NSApplicationDelegate, NSWindowDelegate, AVCapturePhotoCaptureDelegate {
    let session = AVCaptureSession()
    let output = AVCapturePhotoOutput()
    let queue = DispatchQueue(label: "butterpaper.signature.camera")
    var window: NSWindow!
    var preview: AVCaptureVideoPreviewLayer!
    var capture: NSButton!
    var status: NSTextField!
    var finished = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 640, height: 480), styleMask: [.titled, .closable], backing: .buffered, defer: false)
        window.title = "Capture signature"
        window.delegate = self
        let content = window.contentView!
        let view = NSView(frame: NSRect(x: 16, y: 64, width: 608, height: 400))
        view.wantsLayer = true
        preview = AVCaptureVideoPreviewLayer(session: session)
        preview.frame = view.bounds
        preview.videoGravity = .resizeAspect
        view.layer?.addSublayer(preview)
        content.addSubview(view)
        status = NSTextField(labelWithString: "Hold your signature on plain paper in front of the camera.")
        status.frame = NSRect(x: 16, y: 32, width: 460, height: 24)
        content.addSubview(status)
        capture = NSButton(title: "Capture", target: self, action: #selector(takePhoto))
        capture.frame = NSRect(x: 520, y: 16, width: 104, height: 32)
        capture.bezelStyle = .rounded
        capture.isEnabled = false
        capture.keyEquivalent = "\r"
        content.addSubview(capture)
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .authorized: configure()
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .video) { granted in
                DispatchQueue.main.async { granted ? self.configure() : self.fail("Camera access was denied.") }
            }
        default: fail("Camera access is unavailable. Check Camera in System Settings.")
        }
        // A closed parent pipe also releases the device; no orphan capture session.
        DispatchQueue.global().async {
            _ = FileHandle.standardInput.readDataToEndOfFile()
            DispatchQueue.main.async { self.finish(nil, code: 2) }
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 120) { self.finish(nil, code: 2) }
    }
    func configure() {
        guard !finished else { return }
        queue.async {
            guard let device = AVCaptureDevice.default(for: .video),
                  let input = try? AVCaptureDeviceInput(device: device) else {
                DispatchQueue.main.async { self.fail("No camera is available.") }; return
            }
            self.session.beginConfiguration()
            self.session.sessionPreset = .photo
            guard self.session.canAddInput(input), self.session.canAddOutput(self.output) else {
                self.session.commitConfiguration()
                DispatchQueue.main.async { self.fail("This camera cannot capture a signature.") }; return
            }
            self.session.addInput(input)
            self.session.addOutput(self.output)
            self.session.commitConfiguration()
            self.session.startRunning()
            DispatchQueue.main.async { if !self.finished { self.capture.isEnabled = true } }
        }
    }
    @objc func takePhoto() {
        capture.isEnabled = false
        status.stringValue = "Processing signature…"
        output.capturePhoto(with: AVCapturePhotoSettings(), delegate: self)
    }
    func photoOutput(_ output: AVCapturePhotoOutput, didFinishProcessingPhoto photo: AVCapturePhoto, error: Error?) {
        guard error == nil, let data = photo.fileDataRepresentation(), let image = CIImage(data: data) else {
            DispatchQueue.main.async { self.fail("The camera could not capture an image.") }; return
        }
        let extent = image.extent
        guard extent.width > 0, extent.height > 0, extent.width <= 16384, extent.height <= 16384 else {
            DispatchQueue.main.async { self.fail("The camera image is too large.") }; return
        }
        let scale = min(1, 1600 / max(extent.width, extent.height))
        let resized = image.transformed(by: CGAffineTransform(scaleX: scale, y: scale))
        let context = CIContext(options: [.cacheIntermediates: false])
        guard let png = context.pngRepresentation(of: resized, format: .RGBA8, colorSpace: CGColorSpaceCreateDeviceRGB()), png.count <= 16 * 1024 * 1024 else {
            DispatchQueue.main.async { self.fail("The camera image could not be processed.") }; return
        }
        DispatchQueue.main.async { self.finish(png, code: 0) }
    }
    func fail(_ message: String) {
        // Only fixed application messages cross the diagnostic boundary.
        FileHandle.standardError.write(Data(message.utf8))
        finish(nil, code: 1)
    }
    func finish(_ image: Data?, code: Int32) {
        guard !finished else { return }
        finished = true
        queue.async {
            self.session.stopRunning()
            DispatchQueue.main.async {
                self.preview.session = nil
                if let image { FileHandle.standardOutput.write(image) }
                exit(code)
            }
        }
    }
    func windowShouldClose(_ sender: NSWindow) -> Bool { finish(nil, code: 2); return false }
}
let app = NSApplication.shared
let controller = CameraController()
app.setActivationPolicy(.accessory)
app.delegate = controller
app.run()
