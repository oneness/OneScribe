import Foundation
import Combine

class TranscriptionHistoryManager: ObservableObject {
    static let shared = TranscriptionHistoryManager()

    @Published var transcriptions: [Transcription] = []

    private let maxHistoryCount = 10
    private let userDefaultsKey = "transcriptionHistory"

    private init() {
        loadHistory()
    }

    func add(_ transcription: Transcription) {
        transcriptions.append(transcription)

        // Keep only the last maxHistoryCount transcriptions
        if transcriptions.count > maxHistoryCount {
            transcriptions = Array(transcriptions.suffix(maxHistoryCount))
        }

        saveHistory()
    }

    private func saveHistory() {
        if let encoded = try? JSONEncoder().encode(transcriptions) {
            UserDefaults.standard.set(encoded, forKey: userDefaultsKey)
        }
    }

    private func loadHistory() {
        if let data = UserDefaults.standard.data(forKey: userDefaultsKey),
           let decoded = try? JSONDecoder().decode([Transcription].self, from: data) {
            transcriptions = decoded
        }
    }

    func clearHistory() {
        transcriptions.removeAll()
        saveHistory()
    }
}
