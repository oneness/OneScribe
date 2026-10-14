import Foundation

// MARK: - Groq Transcription Service

struct GroqTranscriber: TranscriptionService {
    private let apiKey: String
    private let baseURL = "https://api.groq.com/openai/v1/audio/transcriptions"
    private let httpClient: HTTPClient

    init(apiKey: String, httpClient: HTTPClient = DefaultHTTPClient()) {
        self.apiKey = apiKey
        self.httpClient = httpClient
    }

    func transcribe(_ audioURL: URL) async throws -> String {
        // Check if API is configured
        guard !apiKey.isEmpty else {
            throw TranscriptionError.authenticationFailed
        }

        // Verify file exists
        guard FileManager.default.fileExists(atPath: audioURL.path) else {
            throw TranscriptionError.fileNotFound
        }

        // Read audio file data
        guard let audioData = try? Data(contentsOf: audioURL) else {
            throw TranscriptionError.invalidAudioFormat
        }

        // Create multipart form data
        let boundary = "Boundary-\(UUID().uuidString)"
        var body = Data()

        // Add file
        body.append("--\(boundary)\r\n".data(using: .utf8)!)
        body.append("Content-Disposition: form-data; name=\"file\"; filename=\"\(audioURL.lastPathComponent)\"\r\n".data(using: .utf8)!)
        body.append("Content-Type: audio/m4a\r\n\r\n".data(using: .utf8)!)
        body.append(audioData)
        body.append("\r\n".data(using: .utf8)!)

        // Add model parameter - using turbo for ~8x faster inference with minimal accuracy loss
        body.append("--\(boundary)\r\n".data(using: .utf8)!)
        body.append("Content-Disposition: form-data; name=\"model\"\r\n\r\n".data(using: .utf8)!)
        body.append("whisper-large-v3-turbo".data(using: .utf8)!)
        body.append("\r\n".data(using: .utf8)!)

        // Close boundary
        body.append("--\(boundary)--\r\n".data(using: .utf8)!)

        // Create request
        guard let url = URL(string: baseURL) else {
            throw TranscriptionError.invalidResponse
        }

        let headers = [
            "Authorization": "Bearer \(apiKey)",
            "Content-Type": "multipart/form-data; boundary=\(boundary)"
        ]

        // Make request
        let responseData = try await httpClient.post(url: url, headers: headers, body: body)

        // Parse response
        guard let json = try? JSONSerialization.jsonObject(with: responseData) as? [String: Any],
              let text = json["text"] as? String,
              !text.isEmpty else {
            throw TranscriptionError.emptyResponse
        }

        return text
    }
}

// MARK: - Shared Singleton

extension GroqTranscriber {
    private static func trimmed(_ value: String?) -> String {
        return (value ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    fileprivate static func configFileURL(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        fileManager: FileManager = .default
    ) -> URL? {
        // Optional override for development and testing.
        let overridePath = trimmed(environment["ONESCRIBE_CONFIG_PATH"])
        if !overridePath.isEmpty {
            let expanded = (overridePath as NSString).expandingTildeInPath
            return URL(fileURLWithPath: expanded)
        }

        guard let appSupportDir = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            return nil
        }

        return appSupportDir
            .appendingPathComponent("OneScribe", isDirectory: true)
            .appendingPathComponent("config.json", isDirectory: false)
    }

    private static func apiKeyFromConfigFile(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        fileManager: FileManager = .default
    ) -> String {
        guard let url = configFileURL(environment: environment, fileManager: fileManager),
              let data = try? Data(contentsOf: url),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return ""
        }

        if let key = json["groq_api_key"] as? String {
            return trimmed(key)
        }
        if let key = json["GROQ_API_KEY"] as? String {
            return trimmed(key)
        }

        return ""
    }

    fileprivate static func resolvedAPIKey(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        fileManager: FileManager = .default
    ) -> String {
        let envKey = trimmed(environment["GROQ_API_KEY"])
        if !envKey.isEmpty {
            return envKey
        }

        return apiKeyFromConfigFile(environment: environment, fileManager: fileManager)
    }

    static var shared: GroqTranscriber {
        let apiKey = resolvedAPIKey()
        return GroqTranscriber(apiKey: apiKey)
    }

    func isConfigured() -> Bool {
        return !apiKey.isEmpty
    }
}

// MARK: - Legacy Support (for backward compatibility)

class LegacyGroqTranscriber {
    private let transcriber: GroqTranscriber

    init() {
        let apiKey = GroqTranscriber.resolvedAPIKey()
        guard !apiKey.isEmpty else {
            fatalError("API key not configured. Set GROQ_API_KEY or create ~/Library/Application Support/OneScribe/config.json with {\"groq_api_key\": \"...\"}.")
        }
        self.transcriber = GroqTranscriber(apiKey: apiKey)
    }

    func transcribe(audioFileURL: URL) async throws -> Transcription {
        let text = try await transcriber.transcribe(audioFileURL)
        return Transcription(text: text)
    }

    func isConfigured() -> Bool {
        return !GroqTranscriber.resolvedAPIKey().isEmpty
    }
}
