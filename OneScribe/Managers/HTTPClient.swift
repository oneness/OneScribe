import Foundation

// MARK: - HTTP Client Protocol

protocol HTTPClient {
    func post(url: URL, headers: [String: String], body: Data) async throws -> Data
}

// MARK: - Default HTTP Client Implementation

class DefaultHTTPClient: HTTPClient {
    func post(url: URL, headers: [String: String], body: Data) async throws -> Data {
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.httpBody = body

        for (key, value) in headers {
            request.setValue(value, forHTTPHeaderField: key)
        }

        let (data, response) = try await URLSession.shared.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse else {
            throw TranscriptionError.invalidResponse
        }

        guard httpResponse.statusCode == 200 else {
            if httpResponse.statusCode == 401 {
                throw TranscriptionError.authenticationFailed
            }
            throw TranscriptionError.networkError
        }

        return data
    }
}
