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
    @Published var showRestartPrompt = false
    @Published var isRestarting = false
    @Published var vaultStatus: VaultStatus?
    @Published var daemonStatus: DaemonStatus?

    private let client = DaemonClient()

    private var rivetDir: String {
        FileManager.default.homeDirectoryForCurrentUser.path + "/.rivet"
    }

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

        // Look for rivetd: in app bundle, cargo target dir, /usr/local/bin, then PATH
        let candidates: [String] = {
            var paths: [String] = []
            // Inside .app bundle (DMG install)
            if let bundlePath = Bundle.main.executableURL?.deletingLastPathComponent().appendingPathComponent("rivetd").path {
                paths.append(bundlePath)
            }
            // Cargo build output (development)
            if let bundleURL = Bundle.main.executableURL {
                // Walk up from the executable to find a Cargo workspace root with target/release/rivetd
                var dir = bundleURL.deletingLastPathComponent()
                for _ in 0..<6 {
                    let candidate = dir.appendingPathComponent("target/release/rivetd").path
                    if FileManager.default.isExecutableFile(atPath: candidate) {
                        paths.append(candidate)
                        break
                    }
                    dir = dir.deletingLastPathComponent()
                }
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
        let stderrPipe = Pipe()
        process.standardError = stderrPipe

        do {
            try process.run()

            // Give daemon a moment, then check if it crashed immediately
            try await Task.sleep(nanoseconds: 300_000_000) // 0.3s
            if !process.isRunning {
                let stderrData = stderrPipe.fileHandleForReading.readDataToEndOfFile()
                let stderrText = String(data: stderrData, encoding: .utf8) ?? ""
                let message = stderrText.isEmpty
                    ? "rivetd exited immediately (code \(process.terminationStatus))"
                    : "rivetd failed: \(stderrText.prefix(500))"
                appState = .daemonOffline
                showError(DaemonClientError.rpcError(code: -1, message: message))
                return
            }

            // Wait for daemon to start with retries
            for attempt in 1...5 {
                try await Task.sleep(nanoseconds: UInt64(attempt) * 500_000_000)
                do {
                    try await client.connect()
                    await refreshStatus()
                    return
                } catch {
                    if !process.isRunning {
                        let stderrData = stderrPipe.fileHandleForReading.readDataToEndOfFile()
                        let stderrText = String(data: stderrData, encoding: .utf8) ?? ""
                        let message = stderrText.isEmpty
                            ? "rivetd exited (code \(process.terminationStatus))"
                            : "rivetd failed: \(stderrText.prefix(500))"
                        appState = .daemonOffline
                        showError(DaemonClientError.rpcError(code: -1, message: message))
                        return
                    }
                }
            }
            appState = .daemonOffline
            showError(DaemonClientError.rpcError(
                code: -1,
                message: "rivetd started but socket connection timed out"
            ))
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

    @AppStorage("terminalEmulator") var terminalEmulator: String = TerminalEmulator.terminalApp.rawValue
    @AppStorage("customTerminalCommand") var customTerminalCommand: String = ""

    func getConnectInfo(for connection: RivetConnection) async -> SshConnectInfo? {
        struct Params: Encodable { let connection_id: UUID }
        do {
            return try await client.call(
                method: "ssh.connect_info",
                params: Params(connection_id: connection.id)
            )
        } catch {
            showError(error)
            return nil
        }
    }

    func openInTerminal(_ connection: RivetConnection) async {
        guard let info = await getConnectInfo(for: connection) else { return }
        let emulator = TerminalEmulator(rawValue: terminalEmulator) ?? .terminalApp
        switch emulator {
        case .terminalApp: openInTerminalApp(info)
        case .iterm2:      openInITerm2(info)
        case .warp:        openInWarp(info)
        case .ghostty:     openInGhostty(info)
        case .axis:        openInAxis(info, connectionName: connection.name)
        case .custom:      openWithCustomCommand(info)
        }
    }

    // MARK: - Terminal Launchers

    private func openInTerminalApp(_ info: SshConnectInfo) {
        let sshCmd = info.sshCommand.replacingOccurrences(of: "\"", with: "\\\"")
        let script = """
        tell application "Terminal"
            activate
            do script "\(sshCmd)"
        end tell
        """
        runAppleScript(script)
    }

    private func openInITerm2(_ info: SshConnectInfo) {
        let sshCmd = info.sshCommand.replacingOccurrences(of: "\"", with: "\\\"")
        let script = """
        tell application "iTerm"
            activate
            create window with default profile command "\(sshCmd)"
        end tell
        """
        runAppleScript(script)
    }

    private func openInWarp(_ info: SshConnectInfo) {
        let sshCmd = info.sshCommand.replacingOccurrences(of: "\"", with: "\\\"")
        let script = """
        tell application "Warp"
            activate
        end tell
        delay 0.5
        tell application "System Events"
            tell process "Warp"
                keystroke "t" using command down
                delay 0.3
                keystroke "\(sshCmd)"
                key code 36
            end tell
        end tell
        """
        runAppleScript(script)
    }

    private func openInGhostty(_ info: SshConnectInfo) {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        process.arguments = ["-a", "Ghostty", "--args", "-e"] + info.sshArgv
        do {
            try process.run()
        } catch {
            showError(DaemonClientError.rpcError(code: -1, message: "Failed to launch Ghostty: \(error)"))
        }
    }

    private func openInAxis(_ info: SshConnectInfo, connectionName: String) {
        let argv = info.sshArgv
        guard let jsonData = try? JSONSerialization.data(withJSONObject: argv),
              let jsonStr = String(data: jsonData, encoding: .utf8) else {
            showError(DaemonClientError.rpcError(code: -1, message: "Failed to serialize SSH argv"))
            return
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = [
            "axis-cli", "raw", "terminal.ensure",
            """
            {"workdesk_id":"rivet","surface_id":1,"kind":"shell","title":"SSH: \(connectionName)","cols":120,"rows":40,"command":\(jsonStr)}
            """
        ]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            showError(DaemonClientError.rpcError(code: -1, message: "Failed to launch axis-cli: \(error)"))
        }
    }

    private func openWithCustomCommand(_ info: SshConnectInfo) {
        var cmd = customTerminalCommand
        cmd = cmd.replacingOccurrences(of: "{SSH_CMD}", with: info.sshCommand)
        cmd = cmd.replacingOccurrences(of: "{HOST}", with: info.host)
        cmd = cmd.replacingOccurrences(of: "{PORT}", with: "\(info.port)")
        cmd = cmd.replacingOccurrences(of: "{USER}", with: info.username)

        guard !cmd.isEmpty else {
            showError(DaemonClientError.rpcError(code: -1, message: "Custom terminal command is empty. Configure it in Settings."))
            return
        }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", cmd]
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            showError(DaemonClientError.rpcError(code: -1, message: "Failed to run custom command: \(error)"))
        }
    }

    private func runAppleScript(_ source: String) {
        if let appleScript = NSAppleScript(source: source) {
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

    func createCredential(name: String, auth: AuthMethod, description: String?) async throws {
        struct Params: Encodable {
            let name: String
            let auth: AuthMethod
            let description: String?
        }
        let _: IdResult = try await client.call(
            method: "cred.create",
            params: Params(name: name, auth: auth, description: description)
        )
        await loadCredentials()
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

    // MARK: - Daemon Lifecycle

    /// Stop the running daemon by reading its PID file and sending SIGTERM.
    func stopDaemon() async {
        await client.disconnect()

        let pidPath = rivetDir + "/rivetd.pid"
        let sockPath = rivetDir + "/rivet.sock"

        // Read PID and kill
        if let pidStr = try? String(contentsOfFile: pidPath, encoding: .utf8).trimmingCharacters(in: .whitespacesAndNewlines),
           let pid = pid_t(pidStr) {
            kill(pid, SIGTERM)

            // Wait up to 3 seconds for socket to disappear
            for _ in 0..<30 {
                if !FileManager.default.fileExists(atPath: sockPath) { break }
                try? await Task.sleep(nanoseconds: 100_000_000) // 100ms
            }
        }

        // Clean up stale files
        try? FileManager.default.removeItem(atPath: sockPath)
        try? FileManager.default.removeItem(atPath: pidPath)
    }

    /// Restart the daemon: stop current instance, launch new binary, reconnect.
    func restartDaemon() async {
        isRestarting = true
        print("[AppViewModel] Restarting daemon due to contract mismatch...")

        await stopDaemon()

        // Brief pause to ensure port/socket is fully released
        try? await Task.sleep(nanoseconds: 300_000_000) // 300ms

        await startDaemon()
        isRestarting = false
    }

    // MARK: - Error Handling

    private func showError(_ error: Error) {
        if error.isContractMismatch {
            print("[AppViewModel] Contract mismatch detected: \(error.localizedDescription)")
            showRestartPrompt = true
        } else {
            errorMessage = error.localizedDescription
            showError = true
        }
    }
}
