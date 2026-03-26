import SwiftUI

struct VaultUnlockView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @State private var password = ""
    @State private var confirmPassword = ""
    @State private var isLoading = false
    @State private var isInitMode = false

    var body: some View {
        VStack(spacing: 24) {
            Image(systemName: "lock.shield")
                .font(.system(size: 60))
                .foregroundColor(.accentColor)

            Text(isInitMode ? "Create Vault" : "Unlock Vault")
                .font(.title2)
                .fontWeight(.semibold)

            VStack(spacing: 12) {
                SecureField("Password", text: $password)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 300)
                    .onSubmit {
                        if !isInitMode {
                            unlock()
                        }
                    }

                if isInitMode {
                    SecureField("Confirm Password", text: $confirmPassword)
                        .textFieldStyle(.roundedBorder)
                        .frame(maxWidth: 300)
                }
            }

            HStack(spacing: 12) {
                Button(isInitMode ? "Initialize" : "Unlock") {
                    if isInitMode {
                        initVault()
                    } else {
                        unlock()
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(password.isEmpty || isLoading || (isInitMode && password != confirmPassword))

                if viewModel.vaultStatus?.initialized == false && !isInitMode {
                    Button("Create New Vault") {
                        isInitMode = true
                    }
                    .buttonStyle(.bordered)
                }

                if isInitMode {
                    Button("Cancel") {
                        isInitMode = false
                        password = ""
                        confirmPassword = ""
                    }
                    .buttonStyle(.bordered)
                }
            }

            if isLoading {
                ProgressView()
                    .controlSize(.small)
            }

            if viewModel.vaultStatus?.initialized == false && !isInitMode {
                Text("No vault found. Create one to get started.")
                    .foregroundColor(.secondary)
                    .font(.caption)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding()
    }

    private func unlock() {
        isLoading = true
        Task {
            await viewModel.unlockVault(password: password)
            isLoading = false
            password = ""
        }
    }

    private func initVault() {
        guard password == confirmPassword else { return }
        isLoading = true
        Task {
            await viewModel.initVault(password: password)
            isLoading = false
            password = ""
            confirmPassword = ""
        }
    }
}
