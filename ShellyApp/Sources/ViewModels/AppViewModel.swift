import Foundation
import SwiftUI

enum AppState {
    case connecting
    case daemonOffline
    case vaultLocked
    case ready
}

@MainActor
class AppViewModel: ObservableObject {
    @Published var appState: AppState = .connecting
    @Published var connections: [ShellyConnection] = []
    @Published var selectedConnection: ShellyConnection?
    @Published var showError = false
    @Published var errorMessage = ""
    @Published var vaultStatus: VaultStatus?
    @Published var daemonStatus: DaemonStatus?

    private let client = DaemonClient()

    func initialize() async {
        do {
            try await client.connect()
            await refreshStatus()
        } catch {
            appState = .daemonOffline
        }
    }

    func refreshStatus() async {
        do {
            let status: DaemonStatus = try await client.call(method: "daemon.status")
            daemonStatus = status

            let vStatus: VaultStatus = try await client.call(method: "vault.status")
            vaultStatus = vStatus

            if !vStatus.initialized || vStatus.locked {
                appState = .vaultLocked
            } else {
                appState = .ready
                await loadConnections()
            }
        } catch {
            showError(error)
        }
    }

    func startDaemon() async {
        // Try to start daemon via shell command
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["shellyd"]
        process.standardOutput = nil
        process.standardError = nil

        do {
            try process.run()
            // Wait for daemon to start
            try await Task.sleep(nanoseconds: 2_000_000_000) // 2 seconds
            await initialize()
        } catch {
            showError(error)
        }
    }

    func unlockVault(password: String) async {
        struct Params: Encodable { let password: String }
        do {
            try await client.callVoid(method: "vault.unlock", params: Params(password: password))
            appState = .ready
            await loadConnections()
        } catch {
            showError(error)
        }
    }

    func initVault(password: String) async {
        struct Params: Encodable { let password: String }
        do {
            try await client.callVoid(method: "vault.init", params: Params(password: password))
            appState = .ready
            await loadConnections()
        } catch {
            showError(error)
        }
    }

    func lockVault() async {
        do {
            try await client.callVoid(method: "vault.lock")
            connections = []
            selectedConnection = nil
            appState = .vaultLocked
        } catch {
            showError(error)
        }
    }

    func loadConnections() async {
        struct Params: Encodable {
            let tag: String?
            let group_id: String?
        }
        do {
            let conns: [ShellyConnection] = try await client.call(
                method: "conn.list",
                params: Params(tag: nil, group_id: nil)
            )
            connections = conns.sorted { $0.name < $1.name }
        } catch {
            showError(error)
        }
    }

    func deleteConnection(_ connection: ShellyConnection) async {
        struct Params: Encodable { let id: UUID; let name: String? }
        do {
            try await client.callVoid(
                method: "conn.delete",
                params: Params(id: connection.id, name: nil)
            )
            connections.removeAll { $0.id == connection.id }
            if selectedConnection?.id == connection.id {
                selectedConnection = nil
            }
        } catch {
            showError(error)
        }
    }

    func execCommand(connectionId: UUID, command: String) async -> SshExecResult? {
        struct Params: Encodable {
            let connection_id: UUID
            let command: String
        }
        do {
            return try await client.call(
                method: "ssh.exec",
                params: Params(connection_id: connectionId, command: command)
            )
        } catch {
            showError(error)
            return nil
        }
    }

    func openInTerminal(_ connection: ShellyConnection) {
        // Build SSH command and open in Terminal.app
        var sshCmd = "ssh"
        if connection.port != 22 {
            sshCmd += " -p \(connection.port)"
        }
        if case .keyFile(let path, _) = connection.auth {
            sshCmd += " -i \(path)"
        }
        sshCmd += " \(connection.username)@\(connection.host)"

        let script = """
        tell application "Terminal"
            activate
            do script "\(sshCmd)"
        end tell
        """

        if let appleScript = NSAppleScript(source: script) {
            var error: NSDictionary?
            appleScript.executeAndReturnError(&error)
            if let error = error {
                showError(DaemonClientError.rpcError(
                    code: -1,
                    message: "AppleScript error: \(error)"
                ))
            }
        }
    }

    private func showError(_ error: Error) {
        errorMessage = error.localizedDescription
        showError = true
    }
}
