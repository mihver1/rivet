import SwiftUI

struct ConnectionListView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @State private var searchText = ""
    @State private var showingAddSheet = false
    @State private var showingQuickCommand = false

    var filteredConnections: [ShellyConnection] {
        if searchText.isEmpty {
            return viewModel.connections
        }
        return viewModel.connections.filter {
            $0.name.localizedCaseInsensitiveContains(searchText) ||
            $0.host.localizedCaseInsensitiveContains(searchText) ||
            $0.tags.contains { $0.localizedCaseInsensitiveContains(searchText) }
        }
    }

    var body: some View {
        NavigationSplitView {
            // Sidebar
            List(filteredConnections, selection: $viewModel.selectedConnection) { conn in
                ConnectionRow(connection: conn)
                    .tag(conn)
                    .contextMenu {
                        Button("Open in Terminal") {
                            viewModel.openInTerminal(conn)
                        }
                        Button("Quick Command...") {
                            viewModel.selectedConnection = conn
                            showingQuickCommand = true
                        }
                        Divider()
                        Button("Delete", role: .destructive) {
                            Task { await viewModel.deleteConnection(conn) }
                        }
                    }
            }
            .searchable(text: $searchText, prompt: "Filter connections")
            .navigationTitle("Connections")
            .toolbar {
                ToolbarItemGroup(placement: .primaryAction) {
                    Button {
                        showingAddSheet = true
                    } label: {
                        Label("Add Connection", systemImage: "plus")
                    }

                    Button {
                        Task { await viewModel.loadConnections() }
                    } label: {
                        Label("Refresh", systemImage: "arrow.clockwise")
                    }

                    Menu {
                        Button("Lock Vault") {
                            Task { await viewModel.lockVault() }
                        }
                        Button("Import SSH Config") {
                            Task { await importSshConfig() }
                        }
                    } label: {
                        Label("More", systemImage: "ellipsis.circle")
                    }
                }
            }
        } detail: {
            if let conn = viewModel.selectedConnection {
                ConnectionDetailView(connection: conn)
            } else {
                Text("Select a connection")
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .sheet(isPresented: $showingAddSheet) {
            AddConnectionView()
        }
        .sheet(isPresented: $showingQuickCommand) {
            if let conn = viewModel.selectedConnection {
                QuickCommandView(connection: conn)
            }
        }
    }

    private func importSshConfig() async {
        struct Params: Encodable {
            let path: String?
        }
        let client = DaemonClient()
        do {
            try await client.connect()
            struct ImportResult: Decodable { let imported: Int }
            let _: ImportResult = try await client.call(
                method: "conn.import",
                params: Params(path: nil)
            )
            await viewModel.loadConnections()
            // Show result count (handled via alert in a real app)
        } catch {
            // Error handled by viewModel
        }
    }
}

struct ConnectionRow: View {
    let connection: ShellyConnection

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(connection.name)
                .fontWeight(.medium)

            HStack(spacing: 4) {
                Text("\(connection.username)@\(connection.host)")
                    .font(.caption)
                    .foregroundColor(.secondary)

                if connection.port != 22 {
                    Text(":\(connection.port)")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
            }

            if !connection.tags.isEmpty {
                HStack(spacing: 4) {
                    ForEach(connection.tags, id: \.self) { tag in
                        Text(tag)
                            .font(.caption2)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 1)
                            .background(Color.accentColor.opacity(0.15))
                            .cornerRadius(4)
                    }
                }
            }
        }
        .padding(.vertical, 2)
    }
}
