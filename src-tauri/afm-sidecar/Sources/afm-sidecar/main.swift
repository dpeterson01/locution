// AFM sidecar — phase-1 spike.
//
// A tiny, dependency-free HTTP server that wraps Apple's on-device
// `SystemLanguageModel` (FoundationModels) behind an OpenAI-compatible
// `/v1/chat/completions` + `/v1/models` surface, so Locution's existing
// OpenAI-compatible client can talk to it with zero new networking code.
//
// Security posture (hard requirements from the design review):
//   * binds loopback only (`requiredInterfaceType = .loopback`)
//   * requires `Authorization: Bearer <token>` on every request; the token is
//     minted by the parent (Locution) and passed via the AFM_SIDECAR_TOKEN env
//     var (never argv, which is visible in `ps`)
//   * emits no CORS headers, so a browser page cannot read responses
//
// Crash isolation: FoundationModels initializes inside THIS process. If Apple's
// framework misbehaves at init, it takes down the sidecar, not Locution — the
// parent can just respawn it. That is the whole point of the sidecar boundary.

import Foundation
import Network
import FoundationModels

let readyPrefix = "AFM_SIDECAR_LISTENING "

// ---- config from environment / args ----------------------------------------

let expectedToken = ProcessInfo.processInfo.environment["AFM_SIDECAR_TOKEN"] ?? ""

if expectedToken.isEmpty {
    FileHandle.standardError.write(Data("refusing to start: AFM_SIDECAR_TOKEN not set\n".utf8))
    exit(2)
}

func parsePort() -> UInt16 {
    let args = CommandLine.arguments
    if let i = args.firstIndex(of: "--port"), i + 1 < args.count, let p = UInt16(args[i + 1]) {
        return p
    }
    return 0 // ephemeral; the chosen port is printed on the readyPrefix line
}
let requestedPort = parsePort()

/// The real context window, keyed off the running OS so it widens automatically
/// on macOS 27 (8K) without a code change. FoundationModels does not expose a
/// public token-limit property today, so we map from the OS major version; if
/// Apple later surfaces a real property, this is the single place to swap it.
func contextWindow() -> Int {
    ProcessInfo.processInfo.operatingSystemVersion.majorVersion >= 27 ? 8192 : 4096
}

// ---- FoundationModels call --------------------------------------------------

struct AFMError: Error { let message: String }

func runModel(system: String?, user: String, temperature: Double?) async -> Result<String, AFMError> {
    let model = SystemLanguageModel.default
    guard case .available = model.availability else {
        return .failure(AFMError(message: "model_unavailable"))
    }

    let session: LanguageModelSession
    if let system, !system.isEmpty {
        session = LanguageModelSession(instructions: system)
    } else {
        session = LanguageModelSession()
    }

    do {
        let options = GenerationOptions(temperature: temperature ?? 0.2)
        let response = try await session.respond(to: user, options: options)
        return .success(response.content)
    } catch {
        return .failure(AFMError(message: "generation_failed: \(error.localizedDescription)"))
    }
}

// ---- minimal HTTP/1.1 connection handler ------------------------------------

// Network delivers a single connection's callbacks serially on `queue`, and each
// instance is touched only by its own callbacks, so unchecked Sendable is safe.
final class HTTPConnection: @unchecked Sendable {
    private let conn: NWConnection
    private var buffer = Data()

    init(_ conn: NWConnection) { self.conn = conn }

    func start(queue: DispatchQueue) {
        conn.stateUpdateHandler = { state in
            if case .failed = state { self.conn.cancel() }
        }
        conn.start(queue: queue)
        receive()
    }

    private func receive() {
        conn.receive(minimumIncompleteLength: 1, maximumLength: 1 << 16) { data, _, isComplete, error in
            if let data, !data.isEmpty {
                self.buffer.append(data)
                if self.handleIfReady() { return }
            }
            if error != nil { self.conn.cancel(); return }
            if isComplete { self.conn.cancel(); return }
            self.receive()
        }
    }

    /// Returns true once a full request (headers + Content-Length body) has been
    /// parsed and routed; false if more bytes are still needed.
    private func handleIfReady() -> Bool {
        guard let sep = buffer.range(of: Data("\r\n\r\n".utf8)) else { return false }
        let headerData = buffer.subdata(in: 0..<sep.lowerBound)
        guard let headerStr = String(data: headerData, encoding: .utf8) else {
            send(status: 400, json: ["error": ["message": "bad headers"]]); return true
        }

        let lines = headerStr.components(separatedBy: "\r\n")
        guard let requestLine = lines.first else {
            send(status: 400, json: ["error": ["message": "bad request line"]]); return true
        }
        let parts = requestLine.split(separator: " ")
        guard parts.count >= 2 else {
            send(status: 400, json: ["error": ["message": "bad request line"]]); return true
        }
        let method = String(parts[0])
        let path = String(parts[1])

        var headers: [String: String] = [:]
        for line in lines.dropFirst() {
            guard let idx = line.firstIndex(of: ":") else { continue }
            let key = line[..<idx].trimmingCharacters(in: .whitespaces).lowercased()
            let value = line[line.index(after: idx)...].trimmingCharacters(in: .whitespaces)
            headers[key] = value
        }

        let contentLength = Int(headers["content-length"] ?? "0") ?? 0
        let bodyStart = sep.upperBound
        if buffer.count - bodyStart < contentLength { return false } // need more bytes
        let body = buffer.subdata(in: bodyStart..<(bodyStart + contentLength))

        route(method: method, path: path, headers: headers, body: body)
        return true
    }

    private func route(method: String, path: String, headers: [String: String], body: Data) {
        // Auth gate — every request must carry the session bearer token.
        guard headers["authorization"] == "Bearer \(expectedToken)" else {
            send(status: 401, json: ["error": ["message": "unauthorized"]]); return
        }

        switch (method, path) {
        case ("GET", "/v1/models"):
            send(status: 200, json: [
                "object": "list",
                "data": [[
                    "id": "apple-afm",
                    "object": "model",
                    "context_window": contextWindow(),
                ]],
            ])

        case ("POST", "/v1/chat/completions"):
            guard
                let obj = try? JSONSerialization.jsonObject(with: body) as? [String: Any],
                let messages = obj["messages"] as? [[String: Any]]
            else {
                send(status: 400, json: ["error": ["message": "invalid request body"]]); return
            }
            let system = messages
                .filter { ($0["role"] as? String) == "system" }
                .compactMap { $0["content"] as? String }
                .joined(separator: "\n")
            let user = messages
                .filter { ($0["role"] as? String) != "system" }
                .compactMap { $0["content"] as? String }
                .joined(separator: "\n")
            let temperature = obj["temperature"] as? Double

            Task {
                let result = await runModel(
                    system: system.isEmpty ? nil : system,
                    user: user,
                    temperature: temperature
                )
                switch result {
                case let .success(text):
                    self.send(status: 200, json: [
                        "id": "afm-\(UUID().uuidString)",
                        "object": "chat.completion",
                        "model": "apple-afm",
                        "choices": [[
                            "index": 0,
                            "message": ["role": "assistant", "content": text],
                            "finish_reason": "stop",
                        ]],
                    ])
                case let .failure(error):
                    self.send(status: 500, json: ["error": ["message": error.message]])
                }
            }

        default:
            send(status: 404, json: ["error": ["message": "not found"]])
        }
    }

    /// Writes a JSON response with Connection: close and NO CORS headers.
    private func send(status: Int, json: Any) {
        let bodyData = (try? JSONSerialization.data(withJSONObject: json)) ?? Data("{}".utf8)
        var head = "HTTP/1.1 \(status) \(Self.reason(status))\r\n"
        head += "Content-Type: application/json\r\n"
        head += "Content-Length: \(bodyData.count)\r\n"
        head += "Connection: close\r\n\r\n"
        var out = Data(head.utf8)
        out.append(bodyData)
        conn.send(content: out, completion: .contentProcessed { _ in self.conn.cancel() })
    }

    private static func reason(_ status: Int) -> String {
        switch status {
        case 200: return "OK"
        case 400: return "Bad Request"
        case 401: return "Unauthorized"
        case 404: return "Not Found"
        case 500: return "Internal Server Error"
        default: return "Status"
        }
    }
}

// ---- listener ---------------------------------------------------------------

let params = NWParameters.tcp
params.requiredInterfaceType = .loopback // loopback only — never reachable off-box
params.allowLocalEndpointReuse = true

let listener: NWListener
do {
    if requestedPort == 0 {
        listener = try NWListener(using: params)
    } else {
        listener = try NWListener(using: params, on: NWEndpoint.Port(rawValue: requestedPort)!)
    }
} catch {
    FileHandle.standardError.write(Data("failed to create listener: \(error)\n".utf8))
    exit(1)
}

let queue = DispatchQueue(label: "afm.http", attributes: .concurrent)

listener.newConnectionHandler = { conn in
    HTTPConnection(conn).start(queue: queue)
}

listener.stateUpdateHandler = { state in
    switch state {
    case .ready:
        if let port = listener.port?.rawValue {
            // The parent parses this line to learn the ephemeral port.
            print("\(readyPrefix)\(port)")
            fflush(stdout)
        }
    case let .failed(error):
        FileHandle.standardError.write(Data("listener failed: \(error)\n".utf8))
        exit(1)
    default:
        break
    }
}

listener.start(queue: queue)
dispatchMain()
