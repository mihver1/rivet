import SwiftUI

struct AddConnectionView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @Environment(\.dismiss) var dismiss

    @State private var name = ""
    @State private var host = ""
    @State private var port = "22"
    @State private var username = ""
    @State private var authType = "agent"
    @State private var password = ""
    @State private var keyPath = ""
    @State private var keyPassphrase = ""
    @State private var agentSocketPath = ""
    @State private var selectedCredentialId: UUID?
    @State private var tags = ""
    @State private var notes = ""
    @State private var isSubmitting = false
    @State private var errorMessage: String?

    var body: some View {
        VStack(spacing: 0) {
            Text("Add Connection")
                .font(.title2)
                .fontWeight(.semibold)
                .padding()

            Form {
                Section("Connection") {
                    TextField("Name", text: $name)
                    TextField("Host", text: $host)
                    TextField("Port", text: $port)
                    TextField("Username", text: $username)
                }

                Section("Authentication") {
                    Picker("Method", selection: $authType) {
                        Text("SSH Agent").tag("agent")
                        Text("Password").tag("password")
                        Text("Key File").tag("keyfile")
                        if !viewModel.credentials.isEmpty {
                            Divider()
                            Text("Credential Profile").tag("profile")
                        }
                    }

                    switch authType {
                    case "password":
                        SecureField("Password", text: $password)
                    case "keyfile":
                        TextField("Key File Path", text: $keyPath)
                        SecureField("Key Passphrase (optional)", text: $keyPassphrase)
                    case "profile":
                        Picker("Profile", selection: $selectedCredentialId) {
                            Text("Select...").tag(nil as UUID?)
                            ForEach(viewModel.credentials) { cred in
                                Text("\(cred.name) (\(cred.auth.displayName))")
                                    .tag(cred.id as UUID?)
                            }
                        }
                    default:
                        TextField("Agent Socket Path (optional)", text: $agentSocketPath)
                    }
                }

                Section("Optional") {
                    TextField("Tags (comma-separated)", text: $tags)
                    TextField("Notes", text: $notes, axis: .vertical)
                        .lineLimit(3)
                }
            }
            .formStyle(.grouped)

            if let errorMessage {
                Text(errorMessage)
                    .foregroundColor(.red)
                    .font(.caption)
                    .padding(.horizontal)
            }

            HStack {
                Button("Cancel") {
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)

                Spacer()

                Button("Add") {
                    addConnection()
                }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
                .disabled(!isFormValid || isSubmitting)
            }
            .padding()
        }
        .frame(width: 450, height: 520)
        .task {
            await viewModel.loadCredentials()
        }
    }

    private var isFormValid: Bool {
        guard !name.isEmpty, !host.isEmpty, !username.isEmpty else { return false }
        if authType == "profile" && selectedCredentialId == nil { return false }
        return true
    }

    private func addConnection() {
        isSubmitting = true

        let authSource: AuthSource
        switch authType {
        case "password":
            authSource = .inline(.password(password))
        case "keyfile":
            authSource = .inline(.keyFile(
                path: keyPath,
                passphrase: keyPassphrase.isEmpty ? nil : keyPassphrase
            ))
        case "profile":
            guard let credId = selectedCredentialId else { return }
            authSource = .profile(credentialId: credId)
        default:
            authSource = .inline(.agent(socketPath: agentSocketPath.isEmpty ? nil : agentSocketPath))
        }

        let parsedTags = tags
            .split(separator: ",")
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }

        struct CreateParams: Encodable {
            let name: String
            let host: String
            let port: UInt16?
            let username: String
            let auth: AuthSource
            let tags: [String]?
            let notes: String?
        }

        let params = CreateParams(
            name: name,
            host: host,
            port: UInt16(port),
            username: username,
            auth: authSource,
            tags: parsedTags.isEmpty ? nil : parsedTags,
            notes: notes.isEmpty ? nil : notes
        )

        Task {
            let client = DaemonClient()
            do {
                try await client.connect()
                let _: IdResult = try await client.call(method: "conn.create", params: params)
                await viewModel.loadConnections()
                dismiss()
            } catch {
                print("[AddConnection] Error: \(error)")
                errorMessage = "\(error)"
                isSubmitting = false
            }
        }
    }
}
