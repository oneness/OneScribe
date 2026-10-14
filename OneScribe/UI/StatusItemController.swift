import Cocoa
import Combine
import os.log
import AppKit

class StatusItemController: ObservableObject {
    // REMOVED SINGLETON - causing issues
    // static let shared = StatusItemController()

    private let statusItem: NSStatusItem
    private let recordingManager: RecordingManager
    private var cancellables = Set<AnyCancellable>()
    private let logger = OSLog(subsystem: "com.onescribe.app", category: "StatusItemController")
    private let iconSize = NSSize(width: 20, height: 20)

    init(recordingManager: RecordingManager = .shared) {
        print("DEBUG: StatusItemController init() called - THIS PROVES INIT IS RUNNING")
        NSLog("DEBUG: StatusItemController init() called - THIS PROVES INIT IS RUNNING")
        self.recordingManager = recordingManager
        self.statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        print("DEBUG: Created NSStatusItem")
        NSLog("DEBUG: Created NSStatusItem")
        setup()
        print("DEBUG: setup() completed")
        NSLog("DEBUG: setup() completed")
    }

    private func setup() {
        print("DEBUG: StatusItemController setup() called")
        NSLog("DEBUG: StatusItemController setup() called")

        if let button = statusItem.button {
            button.image = statusIcon(for: .idle)
            button.imagePosition = .imageOnly

            // Fallback text if image generation fails
            if button.image == nil {
                button.title = "OS"
            }

            print("DEBUG: Set status item button icon")
            NSLog("DEBUG: Set status item button icon")

            // Make the button clickable to toggle recording
            button.action = #selector(toggleRecording)
            button.target = self
            button.sendAction(on: .leftMouseDown)
        } else {
            print("ERROR: Status item button is nil!")
            NSLog("ERROR: Status item button is nil!")
        }

        // Create menu
        let menu = NSMenu()

        // Add start/stop recording menu item at the top
        // Note: ⌥+Space is handled by HotkeyManager (global hotkey), not menu keyEquivalent
        let toggleItem = NSMenuItem(title: "▶️ Start Recording (⌥ Space)", action: #selector(toggleRecording), keyEquivalent: "")
        toggleItem.target = self
        menu.addItem(toggleItem)

        menu.addItem(NSMenuItem.separator())

        // Add quit item
        let quitItem = NSMenuItem(title: "Quit", action: #selector(quit), keyEquivalent: "q")
        quitItem.target = self
        menu.addItem(quitItem)

        statusItem.menu = menu

        print("DEBUG: Menu set up complete")

        // Observe RecordingManager state changes
        recordingManager.$state
            .sink { [weak self] state in
                self?.setState(state)
            }
            .store(in: &cancellables)

        print("DEBUG: State observation set up")
    }

    // MARK: - Icon Helpers

    private func statusIcon(for state: AppState) -> NSImage? {
        let image = NSImage(size: iconSize)
        image.lockFocus()

        let drawingRect = NSRect(origin: .zero, size: iconSize).insetBy(dx: 0.4, dy: 0.4)
        let backgroundRect = drawingRect.insetBy(dx: 0.45, dy: 0.45)
        drawBackground(in: backgroundRect)
        drawBaseGlyph(in: backgroundRect.insetBy(dx: 2.1, dy: 1.8))

        switch state {
        case .idle:
            break
        case .recording:
            drawRecordingBadge(in: backgroundRect)
        case .transcribing:
            drawTranscribingBadge(in: backgroundRect)
        }

        image.unlockFocus()
        image.isTemplate = false
        return image
    }

    private func drawBackground(in rect: NSRect) {
        let radius = max(3.0, rect.width * 0.24)
        let backgroundPath = NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius)
        NSColor(calibratedWhite: 0.06, alpha: 1.0).setFill()
        backgroundPath.fill()

        NSColor(calibratedWhite: 1.0, alpha: 0.10).setStroke()
        backgroundPath.lineWidth = 0.7
        backgroundPath.stroke()
    }

    private func drawBaseGlyph(in rect: NSRect) {
        let barHeight = max(1.2, rect.height * 0.085)
        let bars: [(y: CGFloat, widthRatio: CGFloat)] = [
            (0.85, 0.48),
            (0.70, 0.74),
            (0.55, 0.40),
            (0.39, 0.64),
            (0.23, 0.86)
        ]

        NSColor.white.setFill()
        for bar in bars {
            let width = rect.width * bar.widthRatio
            let x = rect.midX - width / 2
            let y = rect.minY + rect.height * bar.y - barHeight / 2
            let barRect = NSRect(x: x, y: y, width: width, height: barHeight)
            NSBezierPath(roundedRect: barRect, xRadius: barHeight / 2, yRadius: barHeight / 2).fill()
        }

        let stemPath = NSBezierPath()
        let topY = rect.minY + rect.height * 0.12
        let neckTopY = rect.minY + rect.height * 0.045
        let bottomY = rect.minY + rect.height * 0.0
        let topHalfWidth = rect.width * 0.23
        let neckHalfWidth = rect.width * 0.10

        stemPath.move(to: NSPoint(x: rect.midX - topHalfWidth, y: topY))
        stemPath.line(to: NSPoint(x: rect.midX + topHalfWidth, y: topY))
        stemPath.line(to: NSPoint(x: rect.midX + neckHalfWidth, y: neckTopY))
        stemPath.line(to: NSPoint(x: rect.midX + neckHalfWidth, y: bottomY))
        stemPath.line(to: NSPoint(x: rect.midX - neckHalfWidth, y: bottomY))
        stemPath.line(to: NSPoint(x: rect.midX - neckHalfWidth, y: neckTopY))
        stemPath.close()
        stemPath.fill()
    }

    private func drawRecordingBadge(in rect: NSRect) {
        let diameter = max(4.2, rect.width * 0.26)
        let badgeRect = NSRect(
            x: rect.maxX - diameter - 0.3,
            y: rect.maxY - diameter - 0.3,
            width: diameter,
            height: diameter
        )

        NSColor(calibratedRed: 0.97, green: 0.20, blue: 0.22, alpha: 1.0).setFill()
        NSBezierPath(ovalIn: badgeRect).fill()

        NSColor.white.setStroke()
        let ring = NSBezierPath(ovalIn: badgeRect.insetBy(dx: 0.4, dy: 0.4))
        ring.lineWidth = 0.7
        ring.stroke()
    }

    private func drawTranscribingBadge(in rect: NSRect) {
        let badgeWidth = max(6.0, rect.width * 0.40)
        let badgeHeight = max(4.0, rect.height * 0.22)
        let badgeRect = NSRect(
            x: rect.maxX - badgeWidth - 0.4,
            y: rect.maxY - badgeHeight - 0.5,
            width: badgeWidth,
            height: badgeHeight
        )
        let badgeRadius = badgeHeight / 2
        let badgePath = NSBezierPath(roundedRect: badgeRect, xRadius: badgeRadius, yRadius: badgeRadius)
        NSColor(calibratedWhite: 0.92, alpha: 1.0).setFill()
        badgePath.fill()

        let dotSize = max(0.9, badgeHeight * 0.27)
        let spacing = dotSize * 0.8
        let totalDotsWidth = dotSize * 3 + spacing * 2
        let startX = badgeRect.midX - totalDotsWidth / 2
        let y = badgeRect.midY - dotSize / 2

        for index in 0..<3 {
            let dotRect = NSRect(
                x: startX + CGFloat(index) * (dotSize + spacing),
                y: y,
                width: dotSize,
                height: dotSize
            )
            NSColor(calibratedWhite: 0.15, alpha: 1.0).setFill()
            NSBezierPath(ovalIn: dotRect).fill()
        }
    }

    func setState(_ state: AppState) {
        print("DEBUG: setState called with: \(state.displayName)")

        if let button = statusItem.button {
            button.image = statusIcon(for: state)

            // Fallback text if image generation fails
            if button.image == nil {
                switch state {
                case .idle:
                    button.title = "OS"
                case .recording:
                    button.title = "REC"
                case .transcribing:
                    button.title = "..."
                }
            } else {
                button.title = ""
            }

            print("DEBUG: Button updated for state: \(state.displayName)")
        }

        // Update menu item text
        guard let menu = statusItem.menu,
              let toggleItem = menu.items.first else { return }

        switch state {
        case .idle:
            toggleItem.title = "▶️ Start Recording (⌥ Space)"
        case .recording:
            toggleItem.title = "⏹ Stop Recording (⌥ Space)"
        case .transcribing:
            toggleItem.title = "⏳ Transcribing..."
        }
    }

    @objc private func quit() {
        NSApplication.shared.terminate(nil)
    }

    @objc private func toggleRecording() {
        os_log("Menu bar icon clicked - toggling recording", log: logger, type: .info)
        RecordingManager.shared.toggle()
    }

}

// MARK: - NSMenuItem Extension for Section Headers

extension NSMenuItem {
    static func sectionHeader(title: String) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        return item
    }
}
