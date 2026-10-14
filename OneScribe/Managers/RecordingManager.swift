import Foundation
import AVFoundation
import Combine
import os.log
import Cocoa

class RecordingManager: NSObject, ObservableObject {
    static let shared = RecordingManager()

    @Published var state: AppState = .idle

    private var audioRecorder: AVAudioRecorder?
    private var recordingURL: URL?
    private let logger = OSLog(subsystem: "com.onescribe.app", category: "RecordingManager")

    // Callback for when recording stops with an audio file
    var onRecordingComplete: ((URL) -> Void)?

    private override init() {
        super.init()
        setupAudioSession()
        os_log("RecordingManager initialized", log: logger, type: .info)
    }

    private func setupAudioSession() {
        // Request microphone permission on macOS
        if #available(macOS 10.14, *) {
            switch AVCaptureDevice.authorizationStatus(for: .audio) {
            case .notDetermined:
                os_log("Requesting microphone permission...", log: logger, type: .info)
                AVCaptureDevice.requestAccess(for: .audio) { granted in
                    if granted {
                        os_log("Microphone permission granted", log: self.logger, type: .info)
                    } else {
                        os_log("Microphone permission denied", log: self.logger, type: .error)
                        // Show alert to user
                        DispatchQueue.main.async {
                            let alert = NSAlert()
                            alert.messageText = "Microphone Access Required"
                            alert.informativeText = "OneScribe needs microphone access to record audio. Please grant permission in System Settings > Privacy & Security > Microphone."
                            alert.alertStyle = .warning
                            alert.addButton(withTitle: "Open System Settings")
                            alert.addButton(withTitle: "Cancel")
                            let response = alert.runModal()
                            if response == .alertFirstButtonReturn {
                                NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")!)
                            }
                        }
                    }
                }
            case .denied, .restricted:
                os_log("Microphone permission denied or restricted", log: logger, type: .error)
                DispatchQueue.main.async {
                    let alert = NSAlert()
                    alert.messageText = "Microphone Access Required"
                    alert.informativeText = "OneScribe needs microphone access to record audio. Please grant permission in System Settings > Privacy & Security > Microphone."
                    alert.alertStyle = .warning
                    alert.addButton(withTitle: "Open System Settings")
                    alert.addButton(withTitle: "Cancel")
                    let response = alert.runModal()
                    if response == .alertFirstButtonReturn {
                        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")!)
                    }
                }
            case .authorized:
                os_log("Microphone permission already granted", log: logger, type: .info)
            @unknown default:
                os_log("Unknown microphone permission status", log: logger, type: .error)
            }
        }
        os_log("Audio recording ready on macOS", log: logger, type: .info)
    }

    func startRecording() {
        os_log("Starting audio recording...", log: logger, type: .info)

        // Create temporary file path
        let tempDir = FileManager.default.temporaryDirectory
        let filename = "recording_\(Int(Date().timeIntervalSince1970)).m4a"
        let fileURL = tempDir.appendingPathComponent(filename)

        // Define recording settings - optimized for speed with minimal quality impact
        let settings: [String: Any] = [
            AVFormatIDKey: Int(kAudioFormatMPEG4AAC),
            AVNumberOfChannelsKey: 1,
            AVSampleRateKey: 16000.0,  // Whisper's native rate - faster upload/processing
            AVEncoderAudioQualityKey: AVAudioQuality.medium.rawValue  // Smaller files, minimal accuracy difference
        ]

        do {
            audioRecorder = try AVAudioRecorder(url: fileURL, settings: settings)
            audioRecorder?.delegate = self
            audioRecorder?.record()

            recordingURL = fileURL
            state = .recording

            os_log("Recording started to: %@", log: logger, type: .info, fileURL.path)
        } catch {
            os_log("Failed to start recording: %{public}@", log: logger, type: .error, error.localizedDescription)
        }
    }

    func stopRecording() {
        os_log("Stopping audio recording...", log: logger, type: .info)

        audioRecorder?.stop()

        // Don't set state to .idle here - let the callback handler do it
        // Wait a moment for the recorder to finish
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
            if let url = self?.recordingURL {
                os_log("Recording saved to: %@", log: self?.logger ?? OSLog.default, type: .info, url.path)
                os_log("Calling onRecordingComplete callback", log: self?.logger ?? OSLog.default, type: .info)
                // State will be set to .transcribing by callback, then .idle when done
                self?.onRecordingComplete?(url)
                os_log("onRecordingComplete callback called", log: self?.logger ?? OSLog.default, type: .info)
            }
        }
    }

    func toggle() {
        os_log("toggle() called, state: %@", log: logger, type: .info, state.displayName)
        switch state {
        case .idle:
            startRecording()
        case .recording:
            stopRecording()
        case .transcribing:
            // If transcribing, don't allow toggle
            os_log("Cannot toggle while transcribing", log: logger, type: .info)
        }
    }

    // Set the recording URL (to be called when actual recording is implemented)
    func setRecordingURL(_ url: URL) {
        self.recordingURL = url
    }
}

// MARK: - AVAudioRecorderDelegate

extension RecordingManager: AVAudioRecorderDelegate {
    func audioRecorderDidFinishRecording(_ recorder: AVAudioRecorder, successfully flag: Bool) {
        os_log("Recording finished, success: %@", log: logger, type: .info, flag ? "YES" : "NO")
    }

    func audioRecorderEncodeErrorDidOccur(_ recorder: AVAudioRecorder, error: Error?) {
        if let error = error {
            os_log("Recording error: %{public}@", log: logger, type: .error, error.localizedDescription)
        }
    }
}
