import Foundation

/// Mirror of Rust's Connection type
struct RivetConnection: Codable, Identifiable, Hashable {
    let id: UUID
    var name: String
    var host: String
    var port: UInt16
    var username: String
    var auth: AuthMethod
    var tags: [String]
    var groupIds: [UUID]
    var jumpHost: UUID?
    var options: SshOptions
    var notes: String?
    var createdAt: String
    var updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, host, port, username, auth, tags
        case groupIds = "group_ids"
        case jumpHost = "jump_host"
        case options, notes
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    static func == (lhs: RivetConnection, rhs: RivetConnection) -> Bool {
        lhs.id == rhs.id
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }
}

/// Mirror of Rust's AuthMethod enum
enum AuthMethod: Codable {
    case password(String)
    case privateKey(keyData: [UInt8], passphrase: String?)
    case keyFile(path: String, passphrase: String?)
    case agent(socketPath: String?)
    case certificate(certPath: String, keyPath: String)
    case interactive

    enum CodingKeys: String, CodingKey {
        case type_ = "type"
        case data
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type_ = try container.decode(String.self, forKey: .type_)

        switch type_ {
        case "Password":
            let password = try container.decode(String.self, forKey: .data)
            self = .password(password)
        case "PrivateKey":
            let data = try container.decode(PrivateKeyData.self, forKey: .data)
            self = .privateKey(keyData: data.keyData, passphrase: data.passphrase)
        case "KeyFile":
            let data = try container.decode(KeyFileData.self, forKey: .data)
            self = .keyFile(path: data.path, passphrase: data.passphrase)
        case "Agent":
            // Handle both legacy (no data) and new (data with socket_path) formats
            if let data = try? container.decodeIfPresent(AgentData.self, forKey: .data) {
                self = .agent(socketPath: data.socketPath)
            } else {
                self = .agent(socketPath: nil)
            }
        case "Certificate":
            let data = try container.decode(CertificateData.self, forKey: .data)
            self = .certificate(certPath: data.certPath, keyPath: data.keyPath)
        case "Interactive":
            self = .interactive
        default:
            throw DecodingError.dataCorrupted(
                .init(codingPath: decoder.codingPath,
                      debugDescription: "Unknown auth type: \(type_)"))
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .password(let password):
            try container.encode("Password", forKey: .type_)
            try container.encode(password, forKey: .data)
        case .privateKey(let keyData, let passphrase):
            try container.encode("PrivateKey", forKey: .type_)
            try container.encode(PrivateKeyData(keyData: keyData, passphrase: passphrase), forKey: .data)
        case .keyFile(let path, let passphrase):
            try container.encode("KeyFile", forKey: .type_)
            try container.encode(KeyFileData(path: path, passphrase: passphrase), forKey: .data)
        case .agent(let socketPath):
            try container.encode("Agent", forKey: .type_)
            if socketPath != nil {
                try container.encode(AgentData(socketPath: socketPath), forKey: .data)
            }
        case .certificate(let certPath, let keyPath):
            try container.encode("Certificate", forKey: .type_)
            try container.encode(CertificateData(certPath: certPath, keyPath: keyPath), forKey: .data)
        case .interactive:
            try container.encode("Interactive", forKey: .type_)
        }
    }

    var displayName: String {
        switch self {
        case .password: return "Password"
        case .privateKey: return "Private Key"
        case .keyFile: return "Key File"
        case .agent(let socketPath):
            if let path = socketPath {
                return "SSH Agent (\(path))"
            }
            return "SSH Agent"
        case .certificate: return "Certificate"
        case .interactive: return "Interactive"
        }
    }
}

struct PrivateKeyData: Codable {
    let keyData: [UInt8]
    let passphrase: String?

    enum CodingKeys: String, CodingKey {
        case keyData = "key_data"
        case passphrase
    }
}

struct KeyFileData: Codable {
    let path: String
    let passphrase: String?
}

struct AgentData: Codable {
    let socketPath: String?

    enum CodingKeys: String, CodingKey {
        case socketPath = "socket_path"
    }
}

struct CertificateData: Codable {
    let certPath: String
    let keyPath: String

    enum CodingKeys: String, CodingKey {
        case certPath = "cert_path"
        case keyPath = "key_path"
    }
}

/// Mirror of Rust's SshOptions
struct SshOptions: Codable {
    var keepaliveInterval: UInt32?
    var keepaliveCountMax: UInt32?
    var compression: Bool
    var connectTimeout: UInt32?
    var extraArgs: [String]

    enum CodingKeys: String, CodingKey {
        case keepaliveInterval = "keepalive_interval"
        case keepaliveCountMax = "keepalive_count_max"
        case compression
        case connectTimeout = "connect_timeout"
        case extraArgs = "extra_args"
    }
}

/// Vault status
struct VaultStatus: Codable {
    let initialized: Bool
    let locked: Bool
}

/// Daemon status
struct DaemonStatus: Codable {
    let uptimeSecs: UInt64
    let activeSessions: UInt32
    let activeTunnels: UInt32
    let vaultLocked: Bool

    enum CodingKeys: String, CodingKey {
        case uptimeSecs = "uptime_secs"
        case activeSessions = "active_sessions"
        case activeTunnels = "active_tunnels"
        case vaultLocked = "vault_locked"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        uptimeSecs = try container.decode(UInt64.self, forKey: .uptimeSecs)
        activeSessions = try container.decode(UInt32.self, forKey: .activeSessions)
        activeTunnels = try container.decodeIfPresent(UInt32.self, forKey: .activeTunnels) ?? 0
        vaultLocked = try container.decode(Bool.self, forKey: .vaultLocked)
    }
}

/// Generic OK result
struct OkResult: Codable {
    let ok: Bool
}

/// ID result from conn.create
struct IdResult: Codable {
    let id: UUID
}

/// SSH exec result
struct SshExecResult: Codable {
    let exitCode: Int32
    let stdout: String
    let stderr: String

    enum CodingKeys: String, CodingKey {
        case exitCode = "exit_code"
        case stdout, stderr
    }
}

// MARK: - Groups

/// Mirror of Rust's Group type
struct RivetGroup: Codable, Identifiable, Hashable {
    let id: UUID
    var name: String
    var description: String?
    var color: String?

    static func == (lhs: RivetGroup, rhs: RivetGroup) -> Bool {
        lhs.id == rhs.id
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }
}

// MARK: - Tunnels

/// Mirror of Rust's TunnelInfo type
struct TunnelInfo: Codable, Identifiable {
    let id: UUID
    let connectionId: UUID
    let connectionName: String
    let spec: TunnelSpec
    let active: Bool

    enum CodingKeys: String, CodingKey {
        case id
        case connectionId = "connection_id"
        case connectionName = "connection_name"
        case spec, active
    }
}

/// Mirror of Rust's TunnelSpec enum
enum TunnelSpec: Codable {
    case local(localPort: UInt16, remoteHost: String, remotePort: UInt16)
    case remote(remotePort: UInt16, localHost: String, localPort: UInt16)
    case dynamic(localPort: UInt16)

    enum CodingKeys: String, CodingKey {
        case Local, Remote, Dynamic
    }

    struct LocalData: Codable {
        let localPort: UInt16
        let remoteHost: String
        let remotePort: UInt16
        enum CodingKeys: String, CodingKey {
            case localPort = "local_port"
            case remoteHost = "remote_host"
            case remotePort = "remote_port"
        }
    }

    struct RemoteData: Codable {
        let remotePort: UInt16
        let localHost: String
        let localPort: UInt16
        enum CodingKeys: String, CodingKey {
            case remotePort = "remote_port"
            case localHost = "local_host"
            case localPort = "local_port"
        }
    }

    struct DynamicData: Codable {
        let localPort: UInt16
        enum CodingKeys: String, CodingKey {
            case localPort = "local_port"
        }
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        if let data = try? container.decode(LocalData.self, forKey: .Local) {
            self = .local(localPort: data.localPort, remoteHost: data.remoteHost, remotePort: data.remotePort)
        } else if let data = try? container.decode(RemoteData.self, forKey: .Remote) {
            self = .remote(remotePort: data.remotePort, localHost: data.localHost, localPort: data.localPort)
        } else if let data = try? container.decode(DynamicData.self, forKey: .Dynamic) {
            self = .dynamic(localPort: data.localPort)
        } else {
            throw DecodingError.dataCorrupted(.init(codingPath: decoder.codingPath, debugDescription: "Unknown tunnel spec"))
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .local(let lp, let rh, let rp):
            try container.encode(LocalData(localPort: lp, remoteHost: rh, remotePort: rp), forKey: .Local)
        case .remote(let rp, let lh, let lp):
            try container.encode(RemoteData(remotePort: rp, localHost: lh, localPort: lp), forKey: .Remote)
        case .dynamic(let lp):
            try container.encode(DynamicData(localPort: lp), forKey: .Dynamic)
        }
    }

    var displayString: String {
        switch self {
        case .local(let lp, let rh, let rp):
            return "-L \(lp):\(rh):\(rp)"
        case .remote(let rp, let lh, let lp):
            return "-R \(rp):\(lh):\(lp)"
        case .dynamic(let lp):
            return "-D \(lp)"
        }
    }

    var typeLabel: String {
        switch self {
        case .local: return "Local"
        case .remote: return "Remote"
        case .dynamic: return "Dynamic"
        }
    }
}

// MARK: - Workflows

/// Workflow summary for list view
struct WorkflowSummary: Codable, Identifiable {
    let id: UUID
    let name: String
    let description: String?
    let steps: [WorkflowStepSummary]
    let variables: [String: String]?

    var stepCount: Int { steps.count }
}

struct WorkflowStepSummary: Codable {
    let name: String
}

/// Result of running a workflow
struct WorkflowRunResult: Codable {
    let workflowName: String
    let connectionName: String
    let steps: [StepRunResult]
    let success: Bool
    let totalSteps: Int
    let completedSteps: Int
    let failedSteps: Int

    enum CodingKeys: String, CodingKey {
        case workflowName = "workflow_name"
        case connectionName = "connection_name"
        case steps, success
        case totalSteps = "total_steps"
        case completedSteps = "completed_steps"
        case failedSteps = "failed_steps"
    }
}

struct StepRunResult: Codable, Identifiable {
    var id: String { stepName }
    let stepName: String
    let success: Bool
    let skipped: Bool
    let stdout: String?
    let stderr: String?
    let exitCode: Int32?
    let bytesTransferred: UInt64?
    let error: String?

    enum CodingKeys: String, CodingKey {
        case stepName = "step_name"
        case success, skipped, stdout, stderr
        case exitCode = "exit_code"
        case bytesTransferred = "bytes_transferred"
        case error
    }
}
