import SwiftUI

struct TunnelListView: View {
    @EnvironmentObject var viewModel: AppViewModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Text("Active Tunnels")
                        .font(.title)
                        .fontWeight(.bold)
                    Spacer()
                    Button {
                        Task { await viewModel.loadTunnels() }
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                }

                if viewModel.tunnels.isEmpty {
                    VStack(spacing: 12) {
                        Image(systemName: "network.slash")
                            .font(.system(size: 40))
                            .foregroundColor(.secondary)
                        Text("No active tunnels")
                            .foregroundColor(.secondary)
                        Text("Create tunnels with the CLI:\nshelly tunnel create <connection> -L 8080:localhost:80")
                            .font(.caption)
                            .foregroundColor(.secondary)
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 40)
                } else {
                    ForEach(viewModel.tunnels) { tunnel in
                        TunnelRow(tunnel: tunnel)
                    }
                }
            }
            .padding(20)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task {
            await viewModel.loadTunnels()
        }
    }
}

struct TunnelRow: View {
    @EnvironmentObject var viewModel: AppViewModel
    let tunnel: TunnelInfo

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Text(tunnel.spec.typeLabel)
                        .font(.caption)
                        .fontWeight(.semibold)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(typeColor.opacity(0.15))
                        .foregroundColor(typeColor)
                        .cornerRadius(4)

                    Text(tunnel.spec.displayString)
                        .font(.system(.body, design: .monospaced))
                }

                Text("via \(tunnel.connectionName)")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            Spacer()

            Text(String(tunnel.id.uuidString.prefix(8)))
                .font(.caption)
                .foregroundColor(.secondary)

            Button(role: .destructive) {
                Task { await viewModel.closeTunnel(tunnel) }
            } label: {
                Image(systemName: "xmark.circle")
            }
            .buttonStyle(.borderless)
        }
        .padding(10)
        .background(Color.secondary.opacity(0.05))
        .cornerRadius(8)
    }

    var typeColor: Color {
        switch tunnel.spec {
        case .local: return .blue
        case .remote: return .orange
        case .dynamic: return .purple
        }
    }
}
