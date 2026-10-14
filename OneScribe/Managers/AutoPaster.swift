import Foundation
import Cocoa
import Carbon
import os.log

// MARK: - Auto Paster

class AutoPaster {
    static let shared = AutoPaster()
    private let logger = OSLog(subsystem: "com.onescribe.app", category: "AutoPaster")

    private init() {}

    func paste(_ text: String) {
        os_log("AutoPaster: Attempting to paste text", log: logger, type: .info)

        // Step 1: Copy text to clipboard
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        let copyResult = pasteboard.setString(text, forType: .string)

        guard copyResult else {
            os_log("AutoPaster: Failed to copy to clipboard", log: logger, type: .error)
            return
        }

        os_log("AutoPaster: Text copied to clipboard, simulating Cmd+V", log: logger, type: .info)

        // Step 2: Play beep to alert user
        NSSound.beep()
        os_log("Beep played", log: logger, type: .info)

        // Step 3: Simulate Cmd+V keystroke
        simulateCmdV()

        // Small delay to ensure the paste event is processed
        Thread.sleep(forTimeInterval: 0.1)

        os_log("AutoPaster: Paste complete", log: logger, type: .info)
    }

    private func simulateCmdV() {
        let source = CGEventSource(stateID: .hidSystemState)

        // Key down event for V with Command modifier
        guard let keyDown = CGEvent(
            keyboardEventSource: source,
            virtualKey: CGKeyCode(kVK_ANSI_V),
            keyDown: true
        ) else {
            os_log("AutoPaster: Failed to create key down event", log: logger, type: .error)
            return
        }

        keyDown.flags = .maskCommand
        keyDown.post(tap: .cghidEventTap)

        // Key up event for V with Command modifier
        guard let keyUp = CGEvent(
            keyboardEventSource: source,
            virtualKey: CGKeyCode(kVK_ANSI_V),
            keyDown: false
        ) else {
            os_log("AutoPaster: Failed to create key up event", log: logger, type: .error)
            return
        }

        keyUp.flags = .maskCommand
        keyUp.post(tap: .cghidEventTap)

        os_log("AutoPaster: Cmd+V simulated successfully", log: logger, type: .info)
    }
}
