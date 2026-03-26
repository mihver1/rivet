import Foundation

/// Mirror of Rust's Connection type
struct ShellyConnection: Codable, Identifiable, Hashable {
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

    static func == (lhs: ShellyConnection, rhs: ShellyConnection) -> Bool {
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
    case agent
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
            self = .agent
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
        case .agent:
            try container.encode("Agent", forKey: .type_)
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
        case .agent: return "SSH Agent"
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
