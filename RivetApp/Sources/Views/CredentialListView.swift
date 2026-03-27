import SwiftUI

struct CredentialListView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @State private var showingAddSheet = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Text("Credentials")
                        .font(.title)
                        .fontWeight(.bold)
                    Spacer()

                    Button {
                        showingAddSheet = true
                    } label: {
                        Label("Add", systemImage: "plus")
                    }
                    .buttonStyle(.bordered)

                    Button {
                        Task { await viewModel.loadCredentials() }
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                }

                if viewModel.credentials.isEmpty {
                    VStack(spacing: 12) {
                        Image(systemName: "key")
                            .font(.system(size: 40))
                            .foregroundColor(.secondary)
                        Text("No credential profiles")
                            .foregroundColor(.secondary)
                        Text("Create reusable authentication profiles\nto share across multiple connections.")
                            .font(.caption)
                            .foregroundColor(.secondary)
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 40)
                } else {
                    ForEach(viewModel.credentials) { cred in
                        CredentialRow(credential: cred) {
                            Task { await viewModel.deleteCredential(cred) }
                        }
                    }
                }
            }
            .padding(20)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task {
            await viewModel.loadCredentials()
        }
        .sheet(isPresented: $showingAddSheet) {
            AddCredentialView()
        }
    }
}

struct CredentialRow: View {
    let credential: RivetCredential
    let onDelete: () -> Void

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(credential.name)
                    .fontWeight(.medium)

                if let desc = credential.description, !desc.isEmpty {
                    Text(desc)
                        .font(.caption)
                        .foregroundColor(.secondary)
                }

                Text(credential.auth.displayName)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            Spacer()

            Button(role: .destructive) {
                onDelete()
            } label: {
                Image(systemName: "trash")
            }
            .buttonStyle(.borderless)
        }
        .padding(10)
        .background(Color.secondary.opacity(0.05))
        .cornerRadius(8)
    }
}
