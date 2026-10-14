import Foundation

/// Manages first-launch setup state
class SetupChecker {
    private let userDefaults: UserDefaultsProtocol
    private let setupCompleteKey = "hasCompletedSetup"

    init(userDefaults: UserDefaultsProtocol = UserDefaultsAdapter()) {
        self.userDefaults = userDefaults
    }

    /// Check if the user has completed the initial setup
    func hasCompletedSetup() -> Bool {
        return userDefaults.bool(forKey: setupCompleteKey)
    }

    /// Mark the setup as completed
    func markSetupComplete() {
        userDefaults.set(true, forKey: setupCompleteKey)
    }

    /// Reset setup (for testing purposes)
    func resetSetup() {
        userDefaults.removeObject(forKey: setupCompleteKey)
    }
}

// MARK: - UserDefaults Adapter

/// Adapter to bridge UserDefaults to our protocol
class UserDefaultsAdapter: UserDefaultsProtocol {
    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    func bool(forKey key: String) -> Bool {
        return defaults.bool(forKey: key)
    }

    func set(_ value: Bool, forKey key: String) {
        defaults.set(value, forKey: key)
    }

    func removeObject(forKey key: String) {
        defaults.removeObject(forKey: key)
    }
}

// MARK: - Protocol Definition

protocol UserDefaultsProtocol {
    func bool(forKey: String) -> Bool
    func set(_ value: Bool, forKey key: String)
    func removeObject(forKey: String)
}
