import SwiftUI

enum SidebarSelection: Hashable {
    case connection(ShellyConnection)
    case group(ShellyGroup)
    case tunnels
    case workflows
}

struct ConnectionListView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @State private var searchText = ""
    @State private var showingAddSheet = false
    @State private var showingAddGroupSheet = false
    @State private var showingQuickCommand = false
    @State private var sidebarSelection: SidebarSelection?

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
            List(selection: $sidebarSelection) {
                // Connections
                Section("Connections") {
                    ForEach(filteredConnections) { conn in
                        ConnectionRow(connection: conn)
                            .tag(SidebarSelection.connection(conn))
                            .contextMenu {
                                Button("Open in Terminal") {
                                    viewModel.openInTerminal(conn)
                                }
                                Button("Quick Command...") {
                                    sidebarSelection = .connection(conn)
                                    viewModel.selectedConnection = conn
                                    showingQuickCommand = true
                                }
                                Divider()
                                Button("Delete", role: .destructive) {
                                    Task { await viewModel.deleteConnection(conn) }
                                }
                            }
                    }
                }

                // Groups
                if !viewModel.groups.isEmpty {
                    Section("Groups") {
                        ForEach(viewModel.groups) { group in
                            GroupRow(group: group, memberCount: viewModel.connectionsInGroup(group).count)
                                .tag(SidebarSelection.group(group))
                                .contextMenu {
                                    Button("Delete", role: .destructive) {
                                        Task { await viewModel.deleteGroup(group) }
                                    }
                                }
                        }
                    }
                }

                // Tunnels & Workflows
                Section("Tools") {
                    Label {
                        HStack {
                            Text("Tunnels")
                            Spacer()
                            if !viewModel.tunnels.isEmpty {
                                Text("\(viewModel.tunnels.count)")
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 1)
                                    .background(Color.secondary.opacity(0.15))
                                    .cornerRadius(4)
                            }
                        }
                    } icon: {
                        Image(systemName: "network")
                    }
                    .tag(SidebarSelection.tunnels)

                    Label {
                        HStack {
                            Text("Workflows")
                            Spacer()
                            if !viewModel.workflows.isEmpty {
                                Text("\(viewModel.workflows.count)")
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 1)
                                    .background(Color.secondary.opacity(0.15))
                                    .cornerRadius(4)
                            }
                        }
                    } icon: {
                        Image(systemName: "flowchart")
                    }
                    .tag(SidebarSelection.workflows)
                }
            }
            .searchable(text: $searchText, prompt: "Filter")
            .navigationTitle("Shelly")
            .toolbar {
                ToolbarItemGroup(placement: .primaryAction) {
                    Button {
                        showingAddSheet = true
                    } label: {
                        Label("Add Connection", systemImage: "plus")
                    }

                    Button {
                        Task {
                            await viewModel.loadConnections()
                            await viewModel.loadGroups()
                            await viewModel.loadTunnels()
                            await viewModel.loadWorkflows()
                        }
                    } label: {
                        Label("Refresh", systemImage: "arrow.clockwise")
                    }

                    Menu {
                        Button("New Group...") {
                            showingAddGroupSheet = true
                        }
                        Divider()
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
            switch sidebarSelection {
            case .connection(let conn):
                ConnectionDetailView(connection: conn)
            case .group(let group):
                GroupDetailView(group: group)
            case .tunnels:
                TunnelListView()
            case .workflows:
                WorkflowListView()
            case nil:
                Text("Select an item")
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .onChange(of: sidebarSelection) { _, newValue in
            if case .connection(let conn) = newValue {
                viewModel.selectedConnection = conn
            }
        }
        .sheet(isPresented: $showingAddSheet) {
            AddConnectionView()
        }
        .sheet(isPresented: $showingAddGroupSheet) {
            AddGroupView()
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

struct GroupRow: View {
    let group: ShellyGroup
    let memberCount: Int

    var body: some View {
        HStack {
            if let color = group.color {
                Circle()
                    .fill(colorFromString(color))
                    .frame(width: 8, height: 8)
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(group.name)
                    .fontWeight(.medium)
                if let desc = group.description, !desc.isEmpty {
                    Text(desc)
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
            }
            Spacer()
            Text("\(memberCount)")
                .font(.caption)
                .foregroundColor(.secondary)
        }
    }

    private func colorFromString(_ str: String) -> Color {
        switch str.lowercased() {
        case "red", "#ff0000": return .red
        case "blue", "#0000ff": return .blue
        case "green", "#00ff00": return .green
        case "orange", "#ff8800": return .orange
        case "purple", "#800080": return .purple
        case "yellow", "#ffff00": return .yellow
        default: return .accentColor
        }
    }
}
