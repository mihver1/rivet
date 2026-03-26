import SwiftUI

struct QuickCommandView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @Environment(\.dismiss) var dismiss

    let connection: ShellyConnection

    @State private var command = ""
    @State private var output = ""
    @State private var isRunning = false
    @State private var commandHistory: [String] = []
    @State private var historyIndex: Int? = nil

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Image(systemName: "terminal")
                Text("Quick Command — \(connection.name)")
                    .font(.headline)
                Spacer()
                Button("Close") {
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
            }
            .padding()

            Divider()

            // Output
            ScrollViewReader { proxy in
                ScrollView {
                    Text(output.isEmpty ? "Run a command to see output here." : output)
                        .font(.system(.body, design: .monospaced))
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding()
                        .id("output-bottom")
                }
                .background(Color(nsColor: .textBackgroundColor))
                .onChange(of: output) {
                    proxy.scrollTo("output-bottom", anchor: .bottom)
                }
            }

            Divider()

            // Input
            HStack {
                Text("\(connection.username)@\(connection.host) $")
                    .font(.system(.body, design: .monospaced))
                    .foregroundColor(.secondary)

                TextField("command", text: $command)
                    .font(.system(.body, design: .monospaced))
                    .textFieldStyle(.plain)
                    .onSubmit {
                        runCommand()
                    }
                    .onKeyPress(.upArrow) {
                        navigateHistory(direction: .up)
                        return .handled
                    }
                    .onKeyPress(.downArrow) {
                        navigateHistory(direction: .down)
                        return .handled
                    }

                if isRunning {
                    ProgressView()
                        .controlSize(.small)
                } else {
                    Button {
                        runCommand()
                    } label: {
                        Image(systemName: "play.fill")
                    }
                    .disabled(command.isEmpty)
                }
            }
            .padding()
        }
        .frame(width: 700, height: 500)
    }

    enum HistoryDirection {
        case up, down
    }

    private func navigateHistory(direction: HistoryDirection) {
        guard !commandHistory.isEmpty else { return }

        switch direction {
        case .up:
            if let idx = historyIndex {
                if idx > 0 {
                    historyIndex = idx - 1
                }
            } else {
                historyIndex = commandHistory.count - 1
            }
        case .down:
            if let idx = historyIndex {
                if idx < commandHistory.count - 1 {
                    historyIndex = idx + 1
                } else {
                    historyIndex = nil
                    command = ""
                    return
                }
            }
        }

        if let idx = historyIndex {
            command = commandHistory[idx]
        }
    }

    private func runCommand() {
        let cmd = command.trimmingCharacters(in: .whitespaces)
        guard !cmd.isEmpty else { return }

        commandHistory.append(cmd)
        historyIndex = nil

        output += "$ \(cmd)\n"
        command = ""
        isRunning = true

        Task {
            if let result = await viewModel.execCommand(
                connectionId: connection.id,
                command: cmd
            ) {
                if !result.stdout.isEmpty {
                    output += result.stdout
                    if !result.stdout.hasSuffix("\n") {
                        output += "\n"
                    }
                }
                if !result.stderr.isEmpty {
                    output += result.stderr
                    if !result.stderr.hasSuffix("\n") {
                        output += "\n"
                    }
                }
                if result.exitCode != 0 {
                    output += "[exit code: \(result.exitCode)]\n"
                }
            }
            isRunning = false
        }
    }
}
