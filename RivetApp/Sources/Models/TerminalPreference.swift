import Foundation

enum TerminalEmulator: String, CaseIterable, Identifiable {
    case terminalApp = "terminal"
    case iterm2 = "iterm2"
    case warp = "warp"
    case ghostty = "ghostty"
    case axis = "axis"
    case custom = "custom"

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .terminalApp: return "Terminal.app"
        case .iterm2: return "iTerm2"
        case .warp: return "Warp"
        case .ghostty: return "Ghostty"
        case .axis: return "Axis"
        case .custom: return "Custom Command"
        }
    }
}

struct SshConnectInfo: Codable {
    let host: String
    let port: UInt16
    let username: String
    let keyPath: String?
    let agentSocketPath: String?
    let extraArgs: [String]

    enum CodingKeys: String, CodingKey {
        case host, port, username
        case keyPath = "key_path"
        case agentSocketPath = "agent_socket_path"
        case extraArgs = "extra_args"
    }

    var sshCommand: String {
        sshArgv.joined(separator: " ")
    }

    var sshArgv: [String] {
        var args = ["ssh"]
        if port != 22 { args += ["-p", "\(port)"] }
        if let key = keyPath { args += ["-i", key] }
        if let agent = agentSocketPath { args += ["-o", "IdentityAgent=\(agent)"] }
        args += extraArgs
        args.append("\(username)@\(host)")
        return args
    }
}
