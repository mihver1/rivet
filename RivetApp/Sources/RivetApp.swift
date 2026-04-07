import SwiftUI

@main
struct RivetApp: App {
    @StateObject private var viewModel = AppViewModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(viewModel)
                .tint(RivetBrandPalette.copper)
        }
        .windowStyle(.titleBar)
        .defaultSize(width: 900, height: 600)

        Settings {
            SettingsView()
        }
    }
}

struct ContentView: View {
    @EnvironmentObject var viewModel: AppViewModel

    var body: some View {
        Group {
            switch viewModel.appState {
            case .connecting:
                VStack(spacing: 18) {
                    RivetBrandLockup(badgeSize: 72, showsTagline: false)
                    ProgressView("Connecting to daemon...")
                }
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
        .alert("Daemon Version Mismatch", isPresented: $viewModel.showRestartPrompt) {
            Button("Restart Daemon") {
                Task {
                    await viewModel.restartDaemon()
                }
            }
            .keyboardShortcut(.defaultAction)
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("The daemon is running an outdated version that doesn't match this app. Restart it to apply the latest changes?")
        }
        .overlay {
            if viewModel.isRestarting {
                ZStack {
                    Color.black.opacity(0.3)
                    VStack(spacing: 14) {
                        ProgressView()
                            .controlSize(.large)
                        Text("Restarting daemon…")
                            .font(.headline)
                            .foregroundStyle(.white)
                    }
                    .padding(30)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 16))
                }
                .ignoresSafeArea()
            }
        }
    }
}

struct DaemonOfflineView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @State private var isStarting = false

    var body: some View {
        VStack(spacing: 22) {
            RivetBrandLockup(badgeSize: 84, showsTagline: false)

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
