import Foundation
import Network

/// JSON-RPC 2.0 client for communicating with rivetd via Unix socket.
actor DaemonClient {
    private var connection: NWConnection?
    private var nextId: UInt64 = 1
    private let socketPath: String

    init() {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        self.socketPath = "\(home)/.rivet/rivet.sock"
    }

    // MARK: - Connection Management

    func connect() async throws {
        let endpoint = NWEndpoint.unix(path: socketPath)
        let params = NWParameters()

        let conn = NWConnection(to: endpoint, using: params)
        self.connection = conn

        return try await withCheckedThrowingContinuation { continuation in
            var resumed = false
            conn.stateUpdateHandler = { state in
                guard !resumed else { return }
                switch state {
                case .ready:
                    resumed = true
                    continuation.resume()
                case .failed(let error):
                    resumed = true
                    continuation.resume(throwing: DaemonClientError.connectionFailed(error.localizedDescription))
                case .waiting(let error):
                    // Socket doesn't exist or daemon not running
                    resumed = true
                    conn.cancel()
                    continuation.resume(throwing: DaemonClientError.connectionFailed(error.localizedDescription))
                case .cancelled:
                    resumed = true
                    continuation.resume(throwing: DaemonClientError.connectionCancelled)
                default:
                    break
                }
            }
            conn.start(queue: .global())
        }
    }

    func disconnect() {
        connection?.cancel()
        connection = nil
    }

    var isConnected: Bool {
        guard let conn = connection else { return false }
        return conn.state == .ready
    }

    // MARK: - RPC Calls

    func call<P: Encodable, R: Decodable>(
        method: String,
        params: P
    ) async throws -> R {
        let result = try await callRaw(method: method, params: params)
        let decoder = JSONDecoder()
        return try decoder.decode(R.self, from: result)
    }

    func call<R: Decodable>(method: String) async throws -> R {
        let result = try await callRaw(method: method, params: Optional<Int>.none)
        let decoder = JSONDecoder()
        return try decoder.decode(R.self, from: result)
    }

    func callVoid<P: Encodable>(method: String, params: P) async throws {
        let _: OkResult = try await call(method: method, params: params)
    }

    func callVoid(method: String) async throws {
        let _: OkResult = try await call(method: method)
    }

    // MARK: - Low-level

    private func callRaw<P: Encodable>(method: String, params: P?) async throws -> Data {
        guard let conn = connection, conn.state == .ready else {
            throw DaemonClientError.notConnected
        }

        let id = nextId
        nextId += 1

        // Build JSON-RPC request
        var request: [String: Any] = [
            "jsonrpc": "2.0",
            "method": method,
            "id": id
        ]

        if let params = params {
            let encoder = JSONEncoder()
            let paramsData = try encoder.encode(params)
            let paramsJson = try JSONSerialization.jsonObject(with: paramsData)
            request["params"] = paramsJson
        }

        let requestData = try JSONSerialization.data(withJSONObject: request)
        var message = requestData
        message.append(0x0A) // newline

        // Send
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            conn.send(content: message, completion: .contentProcessed { error in
                if let error = error {
                    continuation.resume(throwing: DaemonClientError.sendFailed(error.localizedDescription))
                } else {
                    continuation.resume()
                }
            })
        }

        // Receive response (read until newline)
        let responseData = try await receiveUntilNewline(conn: conn)

        // Parse JSON-RPC response
        guard let json = try JSONSerialization.jsonObject(with: responseData) as? [String: Any] else {
            throw DaemonClientError.invalidResponse("not a JSON object")
        }

        if let error = json["error"] as? [String: Any] {
            let code = error["code"] as? Int ?? -1
            let message = error["message"] as? String ?? "unknown error"
            throw DaemonClientError.rpcError(code: code, message: message)
        }

        guard let result = json["result"] else {
            throw DaemonClientError.invalidResponse("missing result")
        }

        return try JSONSerialization.data(withJSONObject: result)
    }

    private func receiveUntilNewline(conn: NWConnection) async throws -> Data {
        var buffer = Data()

        while true {
            let chunk: Data = try await withCheckedThrowingContinuation { continuation in
                conn.receive(minimumIncompleteLength: 1, maximumLength: 65536) { data, _, _, error in
                    if let error = error {
                        continuation.resume(throwing: DaemonClientError.receiveFailed(error.localizedDescription))
                    } else if let data = data {
                        continuation.resume(returning: data)
                    } else {
                        continuation.resume(throwing: DaemonClientError.connectionClosed)
                    }
                }
            }

            buffer.append(chunk)

            // Check for newline
            if buffer.contains(0x0A) {
                // Return everything up to the newline
                if let newlineIndex = buffer.firstIndex(of: 0x0A) {
                    return Data(buffer[buffer.startIndex..<newlineIndex])
                }
            }
        }
    }
}

enum DaemonClientError: LocalizedError {
    case notConnected
    case connectionFailed(String)
    case connectionCancelled
    case connectionClosed
    case sendFailed(String)
    case receiveFailed(String)
    case invalidResponse(String)
    case rpcError(code: Int, message: String)

    var errorDescription: String? {
        switch self {
        case .notConnected:
            return "Not connected to daemon"
        case .connectionFailed(let msg):
            return "Connection failed: \(msg)"
        case .connectionCancelled:
            return "Connection cancelled"
        case .connectionClosed:
            return "Connection closed by daemon"
        case .sendFailed(let msg):
            return "Send failed: \(msg)"
        case .receiveFailed(let msg):
            return "Receive failed: \(msg)"
        case .invalidResponse(let msg):
            return "Invalid response: \(msg)"
        case .rpcError(_, let message):
            return message
        }
    }

    /// Whether this error indicates the daemon is running a different protocol version
    /// (e.g., old binary that doesn't understand new methods or params).
    var isContractMismatch: Bool {
        switch self {
        case .rpcError(let code, let message):
            // -32601 = Method not found, -32602 = Invalid params
            if code == -32601 || code == -32602 { return true }
            // Serde deserialization errors from the daemon
            let lower = message.lowercased()
            let mismatchPatterns = [
                "unknown field", "missing field", "invalid type",
                "unknown variant", "deserialize", "expected ",
                "unrecognized field", "no method"
            ]
            return mismatchPatterns.contains { lower.contains($0) }
        case .invalidResponse:
            // Response couldn't be parsed — possible protocol change
            return true
        default:
            return false
        }
    }
}

extension Error {
    /// Check if this error (possibly wrapped) indicates a daemon contract mismatch.
    var isContractMismatch: Bool {
        if let clientError = self as? DaemonClientError {
            return clientError.isContractMismatch
        }
        // Also catch Swift decoding errors from the client side
        // (daemon sent a valid response, but the shape doesn't match our Swift model)
        if self is DecodingError {
            return true
        }
        return false
    }
}
