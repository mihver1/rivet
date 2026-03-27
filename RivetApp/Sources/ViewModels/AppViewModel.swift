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
    @Published var connections: [RivetConnection] = []
    @Published var selectedConnection: RivetConnection?
    @Published var groups: [RivetGroup] = []
    @Published var tunnels: [TunnelInfo] = []
    @Published var workflows: [WorkflowSummary] = []
    @Published var credentials: [RivetCredential] = []
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
            // Daemon not running — try to auto-start it
            await startDaemon()
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
                await loadGroups()
                await loadTunnels()
                await loadWorkflows()
                await loadCredentials()
            }
        } catch {
            showError(error)
        }
    }

    func startDaemon() async {
        appState = .connecting

        // Look for rivetd: first in app bundle, then in /usr/local/bin, then via PATH
        let candidates: [String] = {
            var paths: [String] = []
            // Inside .app bundle (DMG install)
            if let bundlePath = Bundle.main.executableURL?.deletingLastPathComponent().appendingPathComponent("rivetd").path {
                paths.append(bundlePath)
            }
            paths.append("/usr/local/bin/rivetd")
            return paths
        }()

        let rivetdPath = candidates.first { FileManager.default.isExecutableFile(atPath: $0) }

        let process = Process()
        if let path = rivetdPath {
            process.executableURL = URL(fileURLWithPath: path)
        } else {
            // Fallback: try via PATH
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = ["rivetd"]
        }
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice

        do {
            try process.run()

            // Wait for daemon to start with retries
            for attempt in 1...5 {
                try await Task.sleep(nanoseconds: UInt64(attempt) * 500_000_000) // 0.5s, 1s, 1.5s...
                do {
                    try await client.connect()
                    await refreshStatus()
                    return
                } catch {
                    // Retry
                }
            }
            appState = .daemonOffline
        } catch {
            appState = .daemonOffline
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
            groups = []
            tunnels = []
            workflows = []
            credentials = []
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
            let conns: [RivetConnection] = try await client.call(
                method: "conn.list",
                params: Params(tag: nil, group_id: nil)
            )
            connections = conns.sorted { $0.name < $1.name }
        } catch {
            showError(error)
        }
    }

    func deleteConnection(_ connection: RivetConnection) async {
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

    func openInTerminal(_ connection: RivetConnection) {
        // Build SSH command and open in Terminal.app
        var sshCmd = "ssh"
        if connection.port != 22 {
            sshCmd += " -p \(connection.port)"
        }
        if case .inline(let method) = connection.auth,
           case .keyFile(let path, _) = method {
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

    // MARK: - Groups

    func loadGroups() async {
        do {
            let grps: [RivetGroup] = try await client.call(method: "group.list")
            groups = grps.sorted { $0.name < $1.name }
        } catch {
            showError(error)
        }
    }

    func createGroup(name: String, description: String?, color: String?) async {
        struct Params: Encodable { let name: String; let description: String?; let color: String? }
        do {
            let _: IdResult = try await client.call(
                method: "group.create",
                params: Params(name: name, description: description, color: color)
            )
            await loadGroups()
        } catch {
            showError(error)
        }
    }

    func deleteGroup(_ group: RivetGroup) async {
        struct Params: Encodable { let id: UUID; let name: String? }
        do {
            try await client.callVoid(
                method: "group.delete",
                params: Params(id: group.id, name: nil)
            )
            groups.removeAll { $0.id == group.id }
        } catch {
            showError(error)
        }
    }

    func connectionsInGroup(_ group: RivetGroup) -> [RivetConnection] {
        connections.filter { $0.groupIds.contains(group.id) }
    }

    // MARK: - Tunnels

    func loadTunnels() async {
        do {
            let tuns: [TunnelInfo] = try await client.call(method: "tunnel.list")
            tunnels = tuns
        } catch {
            showError(error)
        }
    }

    func closeTunnel(_ tunnel: TunnelInfo) async {
        struct Params: Encodable { let id: UUID }
        do {
            try await client.callVoid(method: "tunnel.close", params: Params(id: tunnel.id))
            tunnels.removeAll { $0.id == tunnel.id }
        } catch {
            showError(error)
        }
    }

    // MARK: - Workflows

    func loadWorkflows() async {
        do {
            let wfs: [WorkflowSummary] = try await client.call(method: "workflow.list")
            workflows = wfs.sorted { $0.name < $1.name }
        } catch {
            showError(error)
        }
    }

    func deleteWorkflow(_ workflow: WorkflowSummary) async {
        struct Params: Encodable { let id: UUID; let name: String? }
        do {
            try await client.callVoid(
                method: "workflow.delete",
                params: Params(id: workflow.id, name: nil)
            )
            workflows.removeAll { $0.id == workflow.id }
        } catch {
            showError(error)
        }
    }

    func runWorkflow(name: String, connectionName: String?, groupName: String?) async -> [WorkflowRunResult]? {
        struct Params: Encodable {
            let workflow_name: String
            let connection_name: String?
            let group_name: String?
            let variables: [String: String]
        }
        do {
            return try await client.call(
                method: "workflow.run",
                params: Params(
                    workflow_name: name,
                    connection_name: connectionName,
                    group_name: groupName,
                    variables: [:]
                )
            )
        } catch {
            showError(error)
            return nil
        }
    }

    // MARK: - Credentials

    func loadCredentials() async {
        do {
            let creds: [RivetCredential] = try await client.call(method: "cred.list")
            credentials = creds.sorted { $0.name < $1.name }
        } catch {
            showError(error)
        }
    }

    func createCredential(name: String, auth: AuthMethod, description: String?) async {
        struct Params: Encodable {
            let name: String
            let auth: AuthMethod
            let description: String?
        }
        do {
            let _: IdResult = try await client.call(
                method: "cred.create",
                params: Params(name: name, auth: auth, description: description)
            )
            await loadCredentials()
        } catch {
            showError(error)
        }
    }

    func deleteCredential(_ credential: RivetCredential) async {
        struct Params: Encodable { let id: UUID; let name: String?; let force: Bool? }
        do {
            try await client.callVoid(
                method: "cred.delete",
                params: Params(id: credential.id, name: nil, force: nil)
            )
            credentials.removeAll { $0.id == credential.id }
        } catch {
            showError(error)
        }
    }

    private func showError(_ error: Error) {
        errorMessage = error.localizedDescription
        showError = true
    }
}
