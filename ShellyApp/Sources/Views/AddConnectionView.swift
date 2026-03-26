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
    @State private var tags = ""
    @State private var notes = ""
    @State private var isSubmitting = false

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
                    }

                    switch authType {
                    case "password":
                        SecureField("Password", text: $password)
                    case "keyfile":
                        TextField("Key File Path", text: $keyPath)
                        SecureField("Key Passphrase (optional)", text: $keyPassphrase)
                    default:
                        EmptyView()
                    }
                }

                Section("Optional") {
                    TextField("Tags (comma-separated)", text: $tags)
                    TextField("Notes", text: $notes, axis: .vertical)
                        .lineLimit(3)
                }
            }
            .formStyle(.grouped)

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
                .disabled(name.isEmpty || host.isEmpty || username.isEmpty || isSubmitting)
            }
            .padding()
        }
        .frame(width: 450, height: 500)
    }

    private func addConnection() {
        isSubmitting = true

        let authMethod: AuthMethod
        switch authType {
        case "password":
            authMethod = .password(password)
        case "keyfile":
            authMethod = .keyFile(
                path: keyPath,
                passphrase: keyPassphrase.isEmpty ? nil : keyPassphrase
            )
        default:
            authMethod = .agent
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
            let auth: AuthMethod
            let tags: [String]?
            let notes: String?
        }

        let params = CreateParams(
            name: name,
            host: host,
            port: UInt16(port),
            username: username,
            auth: authMethod,
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
                isSubmitting = false
            }
        }
    }
}
