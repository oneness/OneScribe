import Foundation
import AppKit
import ApplicationServices

// MARK: - Protocol Definitions

protocol SystemEventMonitor {
    func startMonitoring(_ handler: @escaping (NSEvent) -> Void)
    func stopMonitoring()
}

protocol KeyCodeDetector {
    func isCmdV(_ event: NSEvent) -> Bool
    func isOptionSpace(_ event: NSEvent) -> Bool
}

// MARK: - Default Implementations

class DefaultKeyCodeDetector: KeyCodeDetector {
    // Cmd+V detection (key code 9 with Command modifier)
    func isCmdV(_ event: NSEvent) -> Bool {
        return event.keyCode == 9 && event.modifierFlags.contains(.command)
    }

    // Option+Space detection (key code 49 with Option modifier)
    func isOptionSpace(_ event: NSEvent) -> Bool {
        return event.keyCode == 49 && event.modifierFlags.contains(.option)
    }
}

class NSEventMonitor: SystemEventMonitor {
    private var monitor: AnyObject?

    func startMonitoring(_ handler: @escaping (NSEvent) -> Void) {
        monitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown, handler: handler) as AnyObject
    }

    func stopMonitoring() {
        if let monitor = monitor {
            NSEvent.removeMonitor(monitor)
            self.monitor = nil
        }
    }
}

// MARK: - HotkeyManager

class HotkeyManager {
    static let shared = HotkeyManager()

    private var eventMonitor: SystemEventMonitor
    private let keyCodeDetector: KeyCodeDetector

    // For testing - allows dependency injection
    init(eventMonitor: SystemEventMonitor = NSEventMonitor(),
         keyCodeDetector: KeyCodeDetector = DefaultKeyCodeDetector()) {
        self.eventMonitor = eventMonitor
        self.keyCodeDetector = keyCodeDetector
    }

    private func checkAccessibilityPermission() -> Bool {
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true]
        let accessEnabled = AXIsProcessTrustedWithOptions(options as CFDictionary)

        print("=== Accessibility Permission Check: \(accessEnabled ? "GRANTED" : "DENIED") ===")
        NSLog("=== Accessibility Permission Check: %@ ===", accessEnabled ? "GRANTED" : "DENIED")

        if accessEnabled {
            print("Accessibility permission granted")
            NSLog("Accessibility permission granted")
        } else {
            print("WARNING: Accessibility permission not granted")
            NSLog("WARNING: Accessibility permission not granted")
            // macOS native dialog will be shown automatically by kAXTrustedCheckOptionPrompt
        }

        return accessEnabled
    }

    func setup() {
        print("DEBUG: HotkeyManager.setup() called - checking accessibility permission")
        NSLog("DEBUG: HotkeyManager.setup() called - checking accessibility permission")

        // Check accessibility permission first
        guard checkAccessibilityPermission() else {
            print("WARNING: Cannot start hotkey monitoring without accessibility permission")
            NSLog("WARNING: Cannot start hotkey monitoring without accessibility permission")
            return
        }

        print("DEBUG: Starting global key event monitoring")
        NSLog("DEBUG: Starting global key event monitoring")

        // Start global key event monitoring
        eventMonitor.startMonitoring { [weak self] event in
            print("DEBUG: Event received! keyCode: \(event.keyCode)")
            NSLog("DEBUG: Event received! keyCode: %d", event.keyCode)
            self?.handleEvent(event)
        }

        print("DEBUG: Global monitoring started")
        NSLog("DEBUG: Global monitoring started")
    }

    private func handleEvent(_ event: NSEvent) {
        // Debug: log all key events
        print("DEBUG: Key pressed - keyCode: \(event.keyCode), modifiers: \(event.modifierFlags.rawValue)")
        NSLog("DEBUG: Key pressed - keyCode: %d, modifiers: %lu", event.keyCode, event.modifierFlags.rawValue)

        // Option+Space (primary hotkey) - toggles recording
        if keyCodeDetector.isOptionSpace(event) {
            print("DEBUG: Option+Space detected! Toggling recording")
            NSLog("DEBUG: Option+Space detected! Toggling recording")
            RecordingManager.shared.toggle()
            return
        }

        // Note: Cmd+V is NOT intercepted here anymore to avoid infinite loop with AutoPaster
        // System Cmd+V works normally for pasting from clipboard
    }

    func stopMonitoring() {
        eventMonitor.stopMonitoring()
    }
}
