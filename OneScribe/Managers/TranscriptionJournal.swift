import Foundation
import AppKit
import os.log

// MARK: - Transcription Journal

class TranscriptionJournal {
    static let shared = TranscriptionJournal()

    private let journalDirectory: URL
    private let logger = OSLog(subsystem: "com.onescribe.app", category: "TranscriptionJournal")
    private let dateFormatter: DateFormatter
    private let timeFormatter: DateFormatter

    private init() {
        // Use ~/Documents/OneScribe/ directory
        let documentsPath = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        self.journalDirectory = documentsPath.appendingPathComponent("OneScribe")

        // Date formatter for filenames: YYYY-MM-DD.md
        self.dateFormatter = DateFormatter()
        self.dateFormatter.dateFormat = "yyyy-MM-dd"

        // Time formatter for section headers: h:mm a
        self.timeFormatter = DateFormatter()
        self.timeFormatter.dateFormat = "h:mm a"

        // Create directory if it doesn't exist (after all properties initialized)
        if !FileManager.default.fileExists(atPath: journalDirectory.path) {
            do {
                try FileManager.default.createDirectory(at: journalDirectory, withIntermediateDirectories: true)
                os_log("Created journal directory: %{public}@", log: logger, type: .info, journalDirectory.path)
            } catch {
                os_log("Failed to create journal directory: %{public}@", log: logger, type: .error, error.localizedDescription)
            }
        }

        os_log("TranscriptionJournal initialized", log: logger, type: .info)
    }

    private func todayFileURL() -> URL {
        let today = dateFormatter.string(from: Date())
        return journalDirectory.appendingPathComponent("\(today).md")
    }

    func saveTranscription(_ text: String) {
        let fileURL = todayFileURL()
        let timestamp = timeFormatter.string(from: Date())

        os_log("Saving transcription to: %{public}@", log: logger, type: .info, fileURL.path)

        do {
            // Check if file exists
            let fileExists = FileManager.default.fileExists(atPath: fileURL.path)

            // Build markdown content
            var content = ""

            if fileExists {
                // File exists, read it and append
                let existingContent = try String(contentsOf: fileURL, encoding: .utf8)
                content = existingContent
            } else {
                // New file, add header
                let formattedDate = DateFormatter.localizedString(
                    from: Date(),
                    dateStyle: .long,
                    timeStyle: .none
                )
                content = "# Transcriptions - \(formattedDate)\n\n"
            }

            // Append new transcription entry
            content += "## \(timestamp)\n\n"
            content += "\(text)\n\n"
            content += "---\n\n"

            // Write to file
            try content.write(to: fileURL, atomically: true, encoding: .utf8)

            os_log("Transcription saved successfully", log: logger, type: .info)
        } catch {
            os_log("Failed to save transcription: %{public}@", log: logger, type: .error, error.localizedDescription)
        }
    }

    func openTodayJournal() {
        let fileURL = todayFileURL()

        // If file doesn't exist yet, create it with header
        if !FileManager.default.fileExists(atPath: fileURL.path) {
            let formattedDate = DateFormatter.localizedString(
                from: Date(),
                dateStyle: .long,
                timeStyle: .none
            )
            let header = "# Transcriptions - \(formattedDate)\n\n"
            try? header.write(to: fileURL, atomically: true, encoding: .utf8)
        }

        // Open with default application (usually Markdown editor or text editor)
        NSWorkspace.shared.open(fileURL)
        os_log("Opened journal: %{public}@", log: logger, type: .info, fileURL.path)
    }

    func openJournalFolder() {
        // Open the folder in Finder
        NSWorkspace.shared.open(journalDirectory)
        os_log("Opened journal folder: %{public}@", log: logger, type: .info, journalDirectory.path)
    }
}
