import SwiftUI

struct GroupDetailView: View {
    @EnvironmentObject var viewModel: AppViewModel
    let group: ShellyGroup

    var members: [ShellyConnection] {
        viewModel.connectionsInGroup(group)
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                // Header
                HStack {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(group.name)
                            .font(.title)
                            .fontWeight(.bold)

                        if let desc = group.description, !desc.isEmpty {
                            Text(desc)
                                .foregroundColor(.secondary)
                        }
                    }
                    Spacer()
                    if let color = group.color, !color.isEmpty {
                        Text(color)
                            .font(.caption)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 4)
                            .background(Color.secondary.opacity(0.15))
                            .cornerRadius(6)
                    }
                }

                Divider()

                // Members
                Text("Members (\(members.count))")
                    .font(.headline)

                if members.isEmpty {
                    Text("No connections in this group.")
                        .foregroundColor(.secondary)
                        .padding()
                } else {
                    ForEach(members) { conn in
                        HStack {
                            Image(systemName: "desktopcomputer")
                                .foregroundColor(.secondary)
                            VStack(alignment: .leading, spacing: 1) {
                                Text(conn.name)
                                    .fontWeight(.medium)
                                Text("\(conn.username)@\(conn.host):\(conn.port)")
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                            }
                            Spacer()
                        }
                        .padding(.vertical, 4)
                    }
                }

                Divider()

                // Info
                HStack {
                    Text("ID")
                        .foregroundColor(.secondary)
                    Spacer()
                    Text(group.id.uuidString)
                        .font(.caption)
                        .textSelection(.enabled)
                }
            }
            .padding(20)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}
