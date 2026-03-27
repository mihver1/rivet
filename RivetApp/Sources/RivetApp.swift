import SwiftUI

@main
struct RivetApp: App {
    @StateObject private var viewModel = AppViewModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(viewModel)
        }
        .windowStyle(.titleBar)
        .defaultSize(width: 900, height: 600)
    }
}

struct ContentView: View {
    @EnvironmentObject var viewModel: AppViewModel

    var body: some View {
        Group {
            switch viewModel.appState {
            case .connecting:
                ProgressView("Connecting to daemon...")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .daemonOffline:
                DaemonOfflineView()
            case .vaultLocked:
                VaultUnlockView()
            case .ready:
                ConnectionListView()
            }
        }
        .task {
            await viewModel.initialize()
        }
        .alert("Error", isPresented: $viewModel.showError) {
            Button("OK") {}
        } message: {
            Text(viewModel.errorMessage)
        }
    }
}

struct DaemonOfflineView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @State private var isStarting = false

    var body: some View {
        VStack(spacing: 20) {
            Image(systemName: "server.rack")
                .font(.system(size: 60))
                .foregroundColor(.secondary)

            Text("Daemon Not Running")
                .font(.title2)
                .fontWeight(.semibold)

            Text("Start the Rivet daemon to manage your SSH connections.")
                .foregroundColor(.secondary)

            Button(action: {
                isStarting = true
                Task {
                    await viewModel.startDaemon()
                    isStarting = false
                }
            }) {
                if isStarting {
                    ProgressView()
                        .controlSize(.small)
                } else {
                    Text("Start Daemon")
                }
            }
            .buttonStyle(.borderedProminent)
            .disabled(isStarting)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding()
    }
}
