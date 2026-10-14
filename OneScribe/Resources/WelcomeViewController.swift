import SwiftUI

/// Setup step model
struct SetupStep: Identifiable {
    let id = UUID()
    let title: String
    let description: String
    let icon: String
}

/// Welcome screen shown on first launch
class WelcomeViewController: NSViewController {
    private let setupChecker: SetupChecker

    init(setupChecker: SetupChecker = SetupChecker()) {
        self.setupChecker = setupChecker
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        // Embed SwiftUI view
        let contentView = WelcomeView(setupChecker: setupChecker, onGetStarted: { [weak self] in
            self?.onGetStarted()
        })
        let hostingController = NSHostingController(rootView: contentView)
        view = hostingController.view
    }

    /// Get Started button action
    func onGetStarted() {
        setupChecker.markSetupComplete()
        dismiss(nil)
    }

    /// Setup steps to display
    var setupSteps: [SetupStep] {
        return [
            SetupStep(
                title: "Enable FN Key",
                description: "Go to System Settings → Keyboard → Keyboard Shortcuts → Function Keys and check 'Use F1, F2, etc. as standard function keys'. Alternatively, install Karabiner-Elements to remap FN to F13.",
                icon: "keyboard"
            ),
            SetupStep(
                title: "Set Groq API Key",
                description: "Set GROQ_API_KEY in your shell environment OR create ~/Library/Application Support/OneScribe/config.json with {\"groq_api_key\":\"<YOUR_KEY>\"}. Get your free API key from https://console.groq.com/keys",
                icon: "key.fill"
            ),
            SetupStep(
                title: "Grant Accessibility Permissions",
                description: "Go to System Settings → Privacy & Security → Accessibility and enable OneScribe. This allows the app to use global hotkeys.",
                icon: "hand.raised.fill"
            ),
            SetupStep(
                title: "Grant Microphone Permissions",
                description: "When prompted, allow OneScribe to access your microphone. You can also enable this in System Settings → Privacy & Security → Microphone.",
                icon: "mic.fill"
            )
        ]
    }
}

// MARK: - SwiftUI Welcome View

struct WelcomeView: View {
    let setupChecker: SetupChecker
    let onGetStarted: () -> Void

    var body: some View {
        ScrollView {
            VStack(spacing: 24) {
                // Welcome header
                VStack(spacing: 12) {
                Image(systemName: "mic.fill")
                    .font(.system(size: 64))
                    .foregroundColor(.blue)

                Text("Welcome to OneScribe")
                    .font(.system(size: 28, weight: .bold))

                Text("Complete these setup steps to get started")
                    .font(.system(size: 14))
                    .foregroundColor(.secondary)
            }
            .padding(.bottom, 20)

            // Setup steps
            VStack(alignment: .leading, spacing: 16) {
                SetupStepView(
                    number: 1,
                    title: "Enable FN Key",
                    description: "Go to System Settings → Keyboard → Keyboard Shortcuts → Function Keys and check 'Use F1, F2, etc. as standard function keys'. Alternatively, install Karabiner-Elements to remap FN to F13.",
                    icon: "keyboard"
                )

                Divider()

                SetupStepView(
                    number: 2,
                    title: "Set Groq API Key",
                    description: "Set GROQ_API_KEY in your shell environment OR create ~/Library/Application Support/OneScribe/config.json with {\"groq_api_key\":\"<YOUR_KEY>\"}. Get your free API key from https://console.groq.com/keys",
                    icon: "key.fill"
                )

                Divider()

                SetupStepView(
                    number: 3,
                    title: "Grant Accessibility Permissions",
                    description: "Go to System Settings → Privacy & Security → Accessibility and enable OneScribe. This allows the app to use global hotkeys.",
                    icon: "hand.raised.fill"
                )

                Divider()

                SetupStepView(
                    number: 4,
                    title: "Grant Microphone Permissions",
                    description: "When prompted, allow OneScribe to access your microphone. You can also enable this in System Settings → Privacy & Security → Microphone.",
                    icon: "mic.fill"
                )
            }
            .padding(.horizontal, 32)

            // Get Started button
            Button(action: onGetStarted) {
                Text("Get Started")
                    .font(.system(size: 16, weight: .semibold))
                    .frame(minWidth: 200)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .padding(.top, 32)
            .padding(.bottom, 32)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color(NSColor.windowBackgroundColor))
        }
        .frame(minWidth: 600, minHeight: 500)
    }
}

// MARK: - Setup Step View

struct SetupStepView: View {
    let number: Int
    let title: String
    let description: String
    let icon: String

    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            // Step number
            ZStack {
                Circle()
                    .fill(Color.blue)
                    .frame(width: 32, height: 32)

                Text("\(number)")
                    .font(.system(size: 16, weight: .bold))
                    .foregroundColor(.white)
            }

            // Icon and content
            Image(systemName: icon)
                .font(.system(size: 24))
                .foregroundColor(.blue)
                .frame(width: 32)

            VStack(alignment: .leading, spacing: 6) {
                Text(title)
                    .font(.system(size: 16, weight: .semibold))

                Text(description)
                    .font(.system(size: 13))
                    .foregroundColor(.secondary)
                    .lineLimit(nil)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.vertical, 4)
    }
}

// MARK: - Preview

#Preview {
    WelcomeView(
        setupChecker: SetupChecker(userDefaults: MockUserDefaultsForPreview()),
        onGetStarted: {}
    )
}

// MARK: - Mock for Preview

class MockUserDefaultsForPreview: UserDefaultsProtocol {
    func bool(forKey key: String) -> Bool { false }
    func set(_ value: Bool, forKey key: String) {}
    func removeObject(forKey key: String) {}
}
