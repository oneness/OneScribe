import Foundation

enum AppState {
    case idle
    case recording
    case transcribing

    var displayName: String {
        switch self {
        case .idle:
            return "Idle"
        case .recording:
            return "Recording"
        case .transcribing:
            return "Transcribing"
        }
    }
}
