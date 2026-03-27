import SwiftUI

struct ConnectionDetailView: View {
    @EnvironmentObject var viewModel: AppViewModel
    let connection: RivetConnection
    @State private var showingQuickCommand = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // Header
                HStack {
                    VStack(alignment: .leading) {
                        Text(connection.name)
                            .font(.title)
                            .fontWeight(.bold)

                        Text("\(connection.username)@\(connection.host):\(connection.port)")
                            .font(.subheadline)
                            .foregroundColor(.secondary)
                    }

                    Spacer()

                    HStack(spacing: 8) {
                        Button {
                            viewModel.openInTerminal(connection)
                        } label: {
                            Label("Open in Terminal", systemImage: "terminal")
                        }
                        .buttonStyle(.borderedProminent)

                        Button {
                            showingQuickCommand = true
                        } label: {
                            Label("Quick Command", systemImage: "text.cursor")
                        }
                        .buttonStyle(.bordered)

                        Button {
                            copySshCommand()
                        } label: {
                            Label("Copy SSH", systemImage: "doc.on.doc")
                        }
                        .buttonStyle(.bordered)
                    }
                }

                Divider()

                // Details
                LazyVGrid(columns: [
                    GridItem(.fixed(120), alignment: .trailing),
                    GridItem(.flexible(), alignment: .leading),
                ], alignment: .leading, spacing: 8) {
                    Text("Host")
                        .foregroundColor(.secondary)
                    Text(connection.host)

                    Text("Port")
                        .foregroundColor(.secondary)
                    Text("\(connection.port)")

                    Text("Username")
                        .foregroundColor(.secondary)
                    Text(connection.username)

                    Text("Auth Method")
                        .foregroundColor(.secondary)
                    Text(connection.auth.displayName)

                    Text("ID")
                        .foregroundColor(.secondary)
                    Text(connection.id.uuidString)
                        .font(.caption)
                        .textSelection(.enabled)
                }

                if !connection.tags.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Tags")
                            .foregroundColor(.secondary)
                        HStack {
                            ForEach(connection.tags, id: \.self) { tag in
                                Text(tag)
                                    .padding(.horizontal, 8)
                                    .padding(.vertical, 3)
                                    .background(Color.accentColor.opacity(0.15))
                                    .cornerRadius(6)
                            }
                        }
                    }
                }

                if let notes = connection.notes, !notes.isEmpty {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Notes")
                            .foregroundColor(.secondary)
                        Text(notes)
                            .textSelection(.enabled)
                    }
                }
            }
            .padding()
        }
        .sheet(isPresented: $showingQuickCommand) {
            QuickCommandView(connection: connection)
        }
    }

    private func copySshCommand() {
        var cmd = "ssh"
        if connection.port != 22 {
            cmd += " -p \(connection.port)"
        }
        if case .inline(let method) = connection.auth,
           case .keyFile(let path, _) = method {
            cmd += " -i \(path)"
        }
        cmd += " \(connection.username)@\(connection.host)"

        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(cmd, forType: .string)
    }
}
