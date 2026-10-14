import Foundation

// MARK: - Transcription Service Protocol

protocol TranscriptionService {
    func transcribe(_ audioURL: URL) async throws -> String
}
