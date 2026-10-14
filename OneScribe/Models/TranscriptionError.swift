import Foundation

// MARK: - Transcription Errors

enum TranscriptionError: LocalizedError {
    case networkError
    case emptyResponse
    case invalidResponse
    case authenticationFailed
    case fileNotFound
    case invalidAudioFormat
    case timeout

    var errorDescription: String? {
        switch self {
        case .networkError:
            return "Network error occurred while transcribing audio"
        case .emptyResponse:
            return "Received empty response from transcription service"
        case .invalidResponse:
            return "Received invalid response from transcription service"
        case .authenticationFailed:
            return "Authentication failed. Please check your API key"
        case .fileNotFound:
            return "Audio file not found"
        case .invalidAudioFormat:
            return "Invalid audio format"
        case .timeout:
            return "Transcription timed out after 30 seconds"
        }
    }
}
