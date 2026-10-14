import Cocoa
import Combine
import os.log

class AppDelegate: NSObject, NSApplicationDelegate {
    private var cancellables = Set<AnyCancellable>()
    private let setupChecker = SetupChecker()
    private var statusItemController: StatusItemController?
    private let logger = OSLog(subsystem: "com.onescribe.app", category: "AppDelegate")

    func applicationDidFinishLaunching(_ notification: Notification) {
        print("OneScribe app starting...")
        NSLog("OneScribe app starting...")

        // Skip welcome screen for now - always initialize
        print("Initializing app...")
        NSLog("Initializing app...")
        initializeApp()

        print("OneScribe app started successfully")
        NSLog("OneScribe app started successfully")
    }

    private func showWelcomeScreen() {
        print("First launch detected - showing welcome screen")
        NSLog("First launch detected - showing welcome screen")

        let welcomeVC = WelcomeViewController(setupChecker: setupChecker)

        // Create a window to host the welcome screen
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 700, height: 750),
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.center()
        window.title = "Welcome to OneScribe"
        window.contentViewController = welcomeVC
        window.makeKeyAndOrderFront(nil)

        // Set up a notification observer to detect when setup is complete
        NotificationCenter.default.addObserver(
            forName: NSWindow.willCloseNotification,
            object: window,
            queue: .main
        ) { [weak self] _ in
            if self?.setupChecker.hasCompletedSetup() == true {
                print("Setup completed - initializing app")
                NSLog("Setup completed - initializing app")
                self?.initializeApp()
            }
        }
    }

    private func initializeApp() {
        print("initializeApp() called")
        NSLog("initializeApp() called")

        // Step 1: Setup menu bar UI - create instance directly
        print("Creating StatusItemController instance...")
        NSLog("Creating StatusItemController instance...")
        statusItemController = StatusItemController()
        print("StatusItemController instance created")
        NSLog("StatusItemController instance created")

        // Step 2: Setup global hotkeys (FN key and Cmd+V)
        print("About to call HotkeyManager.shared.setup()")
        NSLog("About to call HotkeyManager.shared.setup()")
        HotkeyManager.shared.setup()
        print("HotkeyManager setup complete")
        NSLog("HotkeyManager setup complete")

        // Step 3: Setup recording workflow
        print("About to setup recording workflow")
        NSLog("About to setup recording workflow")
        setupRecordingWorkflow()
        print("Recording workflow setup complete")
        NSLog("Recording workflow setup complete")
    }

    private func setupRecordingWorkflow() {
        NSLog("setupRecordingWorkflow: Accessing RecordingManager.shared...")

        // Observe RecordingManager state changes
        RecordingManager.shared.$state
            .dropFirst() // Skip initial value
            .sink { [weak self] state in
                self?.handleStateChange(state)
            }
            .store(in: &cancellables)

        // Set up callback when recording stops
        RecordingManager.shared.onRecordingComplete = { [weak self] audioURL in
            self?.handleRecordingComplete(audioURL)
        }

        NSLog("setupRecordingWorkflow: Callback set up complete")
    }

    private func handleStateChange(_ state: AppState) {
        print("App state changed to: \(state.displayName)")
        NSLog("App state changed to: \(state.displayName)")

        // StatusItemController automatically observes RecordingManager state
        // via Combine, so we don't need to manually update it here
    }

    private func handleRecordingComplete(_ audioURL: URL) {
        os_log("Recording complete, starting transcription...", log: logger, type: .info)

        // Update state to transcribing
        RecordingManager.shared.state = .transcribing

        // Run transcription in background with timeout
        Task { @MainActor in
            do {
                os_log("Starting transcription for: %@", log: logger, type: .info, audioURL.path)

                // Add timeout to prevent hanging
                let transcribedText = try await withThrowingTaskGroup(of: String.self) { group in
                    group.addTask {
                        try await GroqTranscriber.shared.transcribe(audioURL)
                    }

                    group.addTask {
                        try await Task.sleep(nanoseconds: 30_000_000_000) // 30 second timeout
                        throw TranscriptionError.timeout
                    }

                    let result = try await group.next()!
                    group.cancelAll()
                    return result
                }

                os_log("Transcription successful: %{public}@", log: logger, type: .info, transcribedText)

                // Step 1: Auto-paste the text (copies to clipboard, beeps, and simulates Cmd+V)
                AutoPaster.shared.paste(transcribedText)
                os_log("Text auto-pasted to active application", log: logger, type: .info)

                // Step 2: Save to journal (persistent markdown file)
                TranscriptionJournal.shared.saveTranscription(transcribedText)
                os_log("Saved to journal", log: logger, type: .info)

                // Step 3: Return to idle state
                RecordingManager.shared.state = .idle
                os_log("State set to idle", log: logger, type: .info)

            } catch {
                os_log("Transcription failed: %{public}@", log: logger, type: .error, error.localizedDescription)

                // Even on error, return to idle state
                RecordingManager.shared.state = .idle
                os_log("State set to idle after error", log: logger, type: .info)
            }
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        // Cleanup
        HotkeyManager.shared.stopMonitoring()
    }

    func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
        return true
    }
}
