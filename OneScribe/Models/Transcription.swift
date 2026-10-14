import Foundation

struct Transcription: Codable, Identifiable {
    let id: UUID
    var text: String
    let timestamp: Date

    init(id: UUID = UUID(), text: String, timestamp: Date = Date()) {
        self.id = id
        self.text = text
        self.timestamp = timestamp
    }
}
