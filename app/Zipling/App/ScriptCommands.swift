import AppKit

/// AppleScript: `compress`, `extract` and `convert`, defined in Zipling.sdef. Each
/// command waits for its job and returns what it made.

/// The direct parameter as file URLs, whether one file or a list was passed.
private func files(_ value: Any?) -> [URL] {
    switch value {
    case let url as URL: [url]
    case let urls as [URL]: urls
    case let list as [Any]: list.compactMap { $0 as? URL }
    default: []
    }
}

private extension NSScriptCommand {
    /// Runs the work while the script waits, then hands back its result or error.
    func run(_ work: @escaping @MainActor () async throws -> Any?) -> Any? {
        suspendExecution()
        nonisolated(unsafe) let command = self
        Task { @MainActor in
            do {
                command.resumeExecution(withResult: try await work())
            } catch {
                command.scriptErrorNumber = -1
                command.scriptErrorString = error.localizedDescription
                command.resumeExecution(withResult: nil)
            }
        }
        return nil
    }

    func format(_ key: String) -> ArchiveFormat? {
        (evaluatedArguments?[key] as? String).flatMap { ArchiveFormat(rawValue: $0.lowercased()) }
    }

    func level(_ key: String) -> Level? {
        (evaluatedArguments?[key] as? String).flatMap { Level(rawValue: $0.lowercased()) }
    }
}

@objc(ZiplingCompressCommand)
final class CompressCommand: NSScriptCommand {
    override func performDefaultImplementation() -> Any? {
        let items = files(directParameter)
        let format = format("format") ?? .zlg, level = level("level")
        return run { try await Automation.compress(items, format: format, level: level) as NSURL }
    }
}

@objc(ZiplingExtractCommand)
final class ExtractCommand: NSScriptCommand {
    override func performDefaultImplementation() -> Any? {
        let archives = files(directParameter)
        let folder = evaluatedArguments?["destination"] as? URL
        let password = evaluatedArguments?["password"] as? String
        return run {
            var placed: [URL] = []
            for archive in archives {
                placed += try await Automation.extract(archive, into: folder, password: password)
            }
            return placed.map { $0 as NSURL }
        }
    }
}

@objc(ZiplingConvertCommand)
final class ConvertCommand: NSScriptCommand {
    override func performDefaultImplementation() -> Any? {
        let archives = files(directParameter)
        let format = format("format") ?? .zlg, level = level("level") ?? .balanced
        let password = evaluatedArguments?["password"] as? String
        return run {
            var made: [URL] = []
            for archive in archives {
                made.append(try await Automation.convert(archive, to: format, level: level, password: password))
            }
            return made.count == 1 ? made[0] as NSURL : made.map { $0 as NSURL }
        }
    }
}
