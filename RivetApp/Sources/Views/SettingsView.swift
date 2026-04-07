import SwiftUI

struct SettingsView: View {
    @AppStorage("terminalEmulator") private var terminalEmulator: String = TerminalEmulator.terminalApp.rawValue
    @AppStorage("customTerminalCommand") private var customTerminalCommand: String = ""

    private var selectedEmulator: TerminalEmulator {
        TerminalEmulator(rawValue: terminalEmulator) ?? .terminalApp
    }

    var body: some View {
        Form {
            Section("Terminal") {
                Picker("Open SSH connections in:", selection: $terminalEmulator) {
                    ForEach(TerminalEmulator.allCases) { emulator in
                        Text(emulator.displayName).tag(emulator.rawValue)
                    }
                }
                .pickerStyle(.radioGroup)

                if selectedEmulator == .custom {
                    TextField("Command template", text: $customTerminalCommand)
                        .textFieldStyle(.roundedBorder)
                    Text("Placeholders: {SSH_CMD}, {HOST}, {PORT}, {USER}")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }

                if selectedEmulator == .axis {
                    Text("Opens a terminal pane in Axis with the SSH command. Requires axisd to be running.")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 450, height: 250)
    }
}
