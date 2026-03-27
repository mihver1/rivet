import Foundation

// MARK: - Credential Model

/// Mirror of Rust's Credential type
struct RivetCredential: Codable, Identifiable, Hashable {
    let id: UUID
    var name: String
    var auth: AuthMethod
    var description: String?
    var createdAt: String
    var updatedAt: String

    enum CodingKeys: String, CodingKey {
        case id, name, auth, description
        case createdAt = "created_at"
        case updatedAt = "updated_at"
    }

    static func == (lhs: RivetCredential, rhs: RivetCredential) -> Bool {
        lhs.id == rhs.id
    }

    func hash(into hasher: inout Hasher) {
        hasher.combine(id)
    }
}

// MARK: - AuthSource

/// Mirror of Rust's AuthSource enum.
/// Wraps AuthMethod with inline vs. profile-reference semantics.
///
/// JSON wire format:
///   Inline:  {"type":"Inline","data":{"type":"Agent","data":{...}}}
///   Profile: {"type":"Profile","data":{"credential_id":"uuid"}}
///   Legacy:  {"type":"Agent","data":{...}}  (bare AuthMethod, no wrapper)
enum AuthSource: Codable {
    case inline(AuthMethod)
    case profile(credentialId: UUID)

    enum CodingKeys: String, CodingKey {
        case type_ = "type"
        case data
    }

    struct ProfileData: Codable {
        let credentialId: UUID

        enum CodingKeys: String, CodingKey {
            case credentialId = "credential_id"
        }
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let typeStr = try container.decode(String.self, forKey: .type_)

        switch typeStr {
        case "Inline":
            let auth = try container.decode(AuthMethod.self, forKey: .data)
            self = .inline(auth)
        case "Profile":
            let data = try container.decode(ProfileData.self, forKey: .data)
            self = .profile(credentialId: data.credentialId)
        default:
            // Legacy format: bare AuthMethod (e.g. {"type":"Agent","data":{...}})
            // Re-decode from the top-level container as AuthMethod
            let auth = try AuthMethod(from: decoder)
            self = .inline(auth)
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .inline(let auth):
            try container.encode("Inline", forKey: .type_)
            try container.encode(auth, forKey: .data)
        case .profile(let credentialId):
            try container.encode("Profile", forKey: .type_)
            try container.encode(ProfileData(credentialId: credentialId), forKey: .data)
        }
    }

    /// User-facing display name for the auth source.
    var displayName: String {
        switch self {
        case .inline(let auth):
            return auth.displayName
        case .profile:
            return "Credential Profile"
        }
    }

    /// Extract the inline AuthMethod, if this is an inline source.
    var inlineMethod: AuthMethod? {
        if case .inline(let auth) = self {
            return auth
        }
        return nil
    }
}
