import Testing
import Foundation
@testable import OneScribe

@Test("AppState display names remain stable")
func appStateDisplayNames() {
    #expect(AppState.idle.displayName == "Idle")
    #expect(AppState.recording.displayName == "Recording")
    #expect(AppState.transcribing.displayName == "Transcribing")
}

@Test("Transcription Codable roundtrip preserves fields")
func transcriptionCodableRoundtrip() throws {
    let original = Transcription(id: UUID(), text: "Hello world", timestamp: Date())
    let encoded = try JSONEncoder().encode(original)
    let decoded = try JSONDecoder().decode(Transcription.self, from: encoded)

    #expect(decoded.id == original.id)
    #expect(decoded.text == original.text)
    #expect(decoded.timestamp == original.timestamp)
}

@Test("GroqTranscriber conforms to TranscriptionService")
func groqTranscriberConformsToProtocol() {
    let transcriber = GroqTranscriber(
        apiKey: "test-api-key",
        httpClient: StubHTTPClient(mode: .success(text: "ok"))
    )
    let service: TranscriptionService = transcriber
    #expect(service is TranscriptionService)
}

@Test("GroqTranscriber returns text on successful response")
func groqTranscriberReturnsTextOnSuccess() async throws {
    let audioFile = try makeTempAudioFile()
    defer { try? FileManager.default.removeItem(at: audioFile) }

    let transcriber = GroqTranscriber(
        apiKey: "test-api-key",
        httpClient: StubHTTPClient(mode: .success(text: "hello from test"))
    )

    let result = try await transcriber.transcribe(audioFile)
    #expect(result == "hello from test")
}

@Test("GroqTranscriber throws authenticationFailed when API key is empty")
func groqTranscriberThrowsAuthenticationFailedWhenApiKeyMissing() async throws {
    let audioFile = try makeTempAudioFile()
    defer { try? FileManager.default.removeItem(at: audioFile) }

    let transcriber = GroqTranscriber(
        apiKey: "",
        httpClient: StubHTTPClient(mode: .success(text: "unused"))
    )

    await #expect(throws: TranscriptionError.authenticationFailed) {
        _ = try await transcriber.transcribe(audioFile)
    }
}

@Test("GroqTranscriber throws fileNotFound for missing file path")
func groqTranscriberThrowsFileNotFoundForMissingFile() async throws {
    let nonExistentURL = URL(fileURLWithPath: "/tmp/non-existent-\(UUID().uuidString).m4a")
    let transcriber = GroqTranscriber(
        apiKey: "test-api-key",
        httpClient: StubHTTPClient(mode: .success(text: "unused"))
    )

    await #expect(throws: TranscriptionError.fileNotFound) {
        _ = try await transcriber.transcribe(nonExistentURL)
    }
}

@Test("GroqTranscriber propagates network errors from HTTPClient")
func groqTranscriberPropagatesNetworkErrors() async throws {
    let audioFile = try makeTempAudioFile()
    defer { try? FileManager.default.removeItem(at: audioFile) }

    let transcriber = GroqTranscriber(
        apiKey: "test-api-key",
        httpClient: StubHTTPClient(mode: .error(TranscriptionError.networkError))
    )

    await #expect(throws: TranscriptionError.networkError) {
        _ = try await transcriber.transcribe(audioFile)
    }
}

@Test("GroqTranscriber throws emptyResponse for malformed JSON payload")
func groqTranscriberThrowsEmptyResponseForMalformedPayload() async throws {
    let audioFile = try makeTempAudioFile()
    defer { try? FileManager.default.removeItem(at: audioFile) }

    let transcriber = GroqTranscriber(
        apiKey: "test-api-key",
        httpClient: StubHTTPClient(mode: .rawJSON(["notText": "value"]))
    )

    await #expect(throws: TranscriptionError.emptyResponse) {
        _ = try await transcriber.transcribe(audioFile)
    }
}

@Test("GroqTranscriber throws emptyResponse for empty text")
func groqTranscriberThrowsEmptyResponseForEmptyText() async throws {
    let audioFile = try makeTempAudioFile()
    defer { try? FileManager.default.removeItem(at: audioFile) }

    let transcriber = GroqTranscriber(
        apiKey: "test-api-key",
        httpClient: StubHTTPClient(mode: .success(text: ""))
    )

    await #expect(throws: TranscriptionError.emptyResponse) {
        _ = try await transcriber.transcribe(audioFile)
    }
}

// MARK: - Test Support

private func makeTempAudioFile() throws -> URL {
    let tempDir = FileManager.default.temporaryDirectory
    let fileURL = tempDir.appendingPathComponent("test-audio-\(UUID().uuidString).m4a")
    try Data("fake audio bytes".utf8).write(to: fileURL)
    return fileURL
}

private final class StubHTTPClient: HTTPClient {
    enum Mode {
        case success(text: String)
        case rawJSON([String: Any])
        case error(Error)
    }

    let mode: Mode

    init(mode: Mode) {
        self.mode = mode
    }

    func post(url: URL, headers: [String: String], body: Data) async throws -> Data {
        switch mode {
        case .success(let text):
            return try JSONSerialization.data(withJSONObject: ["text": text])
        case .rawJSON(let payload):
            return try JSONSerialization.data(withJSONObject: payload)
        case .error(let error):
            throw error
        }
    }
}
