import SwiftUI

struct AddCredentialView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var description = ""
    @State private var authType = "agent"
    @State private var password = ""
    @State private var keyPath = ""
    @State private var keyPassphrase = ""
    @State private var agentSocketPath = ""
    @State private var isSubmitting = false
    @State private var errorMessage: String?

    var body: some View {
        VStack(spacing: 0) {
            Text("New Credential Profile")
                .font(.title2)
                .fontWeight(.semibold)
                .padding()

            Form {
                Section("Profile") {
                    TextField("Name", text: $name)
                    TextField("Description (optional)", text: $description)
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
                        TextField("Agent Socket Path (optional)", text: $agentSocketPath)
                    }
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

                Button("Create") {
                    createCredential()
                }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
                .disabled(name.isEmpty || isSubmitting)
            }
            .padding()
        }
        .frame(width: 450, height: 420)
    }

    private func createCredential() {
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
            authMethod = .agent(socketPath: agentSocketPath.isEmpty ? nil : agentSocketPath)
        }

        Task {
            do {
                try await viewModel.createCredential(
                    name: name,
                    auth: authMethod,
                    description: description.isEmpty ? nil : description
                )
                dismiss()
            } catch {
                print("[AddCredential] Error: \(error)")
                errorMessage = "\(error)"
                isSubmitting = false
            }
        }
    }
}
