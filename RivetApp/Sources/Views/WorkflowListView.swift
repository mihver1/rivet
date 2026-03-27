import SwiftUI

struct WorkflowListView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @State private var showingRunSheet = false
    @State private var selectedWorkflow: WorkflowSummary?
    @State private var runResults: [WorkflowRunResult]?
    @State private var showingResults = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Text("Workflows")
                        .font(.title)
                        .fontWeight(.bold)
                    Spacer()
                    Button {
                        Task { await viewModel.loadWorkflows() }
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                }

                if viewModel.workflows.isEmpty {
                    VStack(spacing: 12) {
                        Image(systemName: "flowchart")
                            .font(.system(size: 40))
                            .foregroundColor(.secondary)
                        Text("No workflows")
                            .foregroundColor(.secondary)
                        Text("Import workflows with the CLI:\nrivet workflow import deploy.yaml")
                            .font(.caption)
                            .foregroundColor(.secondary)
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 40)
                } else {
                    ForEach(viewModel.workflows) { wf in
                        WorkflowRow(workflow: wf) {
                            selectedWorkflow = wf
                            showingRunSheet = true
                        } onDelete: {
                            Task { await viewModel.deleteWorkflow(wf) }
                        }
                    }
                }
            }
            .padding(20)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task {
            await viewModel.loadWorkflows()
        }
        .sheet(isPresented: $showingRunSheet) {
            if let wf = selectedWorkflow {
                RunWorkflowView(workflow: wf)
            }
        }
    }
}

struct WorkflowRow: View {
    let workflow: WorkflowSummary
    let onRun: () -> Void
    let onDelete: () -> Void

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text(workflow.name)
                    .fontWeight(.medium)

                if let desc = workflow.description, !desc.isEmpty {
                    Text(desc)
                        .font(.caption)
                        .foregroundColor(.secondary)
                }

                HStack(spacing: 8) {
                    Text("\(workflow.stepCount) steps")
                        .font(.caption)
                        .foregroundColor(.secondary)

                    if let vars = workflow.variables, !vars.isEmpty {
                        Text("\(vars.count) variables")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
            }

            Spacer()

            Button("Run") {
                onRun()
            }
            .buttonStyle(.bordered)

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

struct RunWorkflowView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @Environment(\.dismiss) private var dismiss

    let workflow: WorkflowSummary

    @State private var targetType: TargetType = .connection
    @State private var selectedConnectionName = ""
    @State private var selectedGroupName = ""
    @State private var isRunning = false
    @State private var results: [WorkflowRunResult]?

    enum TargetType: String, CaseIterable {
        case connection = "Connection"
        case group = "Group"
    }

    var body: some View {
        VStack(spacing: 0) {
            Text("Run \(workflow.name)")
                .font(.headline)
                .padding()

            if let results = results {
                // Show results
                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        ForEach(Array(results.enumerated()), id: \.offset) { _, result in
                            WorkflowResultView(result: result)
                        }
                    }
                    .padding()
                }

                HStack {
                    Spacer()
                    Button("Done") { dismiss() }
                        .buttonStyle(.borderedProminent)
                }
                .padding()
            } else {
                // Target selection
                Form {
                    Picker("Target", selection: $targetType) {
                        ForEach(TargetType.allCases, id: \.self) { t in
                            Text(t.rawValue).tag(t)
                        }
                    }

                    if targetType == .connection {
                        Picker("Connection", selection: $selectedConnectionName) {
                            Text("Select...").tag("")
                            ForEach(viewModel.connections) { conn in
                                Text(conn.name).tag(conn.name)
                            }
                        }
                    } else {
                        Picker("Group", selection: $selectedGroupName) {
                            Text("Select...").tag("")
                            ForEach(viewModel.groups) { group in
                                Text(group.name).tag(group.name)
                            }
                        }
                    }
                }
                .padding()

                HStack {
                    Button("Cancel") { dismiss() }
                        .keyboardShortcut(.cancelAction)

                    Spacer()

                    if isRunning {
                        ProgressView()
                            .controlSize(.small)
                            .padding(.trailing, 8)
                    }

                    Button("Run") {
                        isRunning = true
                        Task {
                            let connName = targetType == .connection ? selectedConnectionName : nil
                            let grpName = targetType == .group ? selectedGroupName : nil
                            results = await viewModel.runWorkflow(
                                name: workflow.name,
                                connectionName: connName,
                                groupName: grpName
                            )
                            isRunning = false
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(isRunning || !isTargetSelected)
                }
                .padding()
            }
        }
        .frame(width: 550, height: 450)
    }

    var isTargetSelected: Bool {
        switch targetType {
        case .connection: return !selectedConnectionName.isEmpty
        case .group: return !selectedGroupName.isEmpty
        }
    }
}

struct WorkflowResultView: View {
    let result: WorkflowRunResult

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Image(systemName: result.success ? "checkmark.circle.fill" : "xmark.circle.fill")
                    .foregroundColor(result.success ? .green : .red)
                Text(result.connectionName)
                    .fontWeight(.medium)
                Spacer()
                Text("\(result.completedSteps)/\(result.totalSteps) steps")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            ForEach(result.steps) { step in
                HStack(alignment: .top, spacing: 6) {
                    Image(systemName: stepIcon(step))
                        .font(.caption)
                        .foregroundColor(stepColor(step))
                        .frame(width: 14)

                    VStack(alignment: .leading, spacing: 2) {
                        Text(step.stepName)
                            .font(.caption)
                            .fontWeight(.medium)

                        if let stdout = step.stdout, !stdout.isEmpty {
                            Text(stdout.trimmingCharacters(in: .whitespacesAndNewlines))
                                .font(.system(.caption2, design: .monospaced))
                                .foregroundColor(.secondary)
                                .lineLimit(3)
                        }

                        if let err = step.error {
                            Text(err)
                                .font(.caption2)
                                .foregroundColor(.red)
                        }
                    }
                }
            }
        }
        .padding(10)
        .background(Color.secondary.opacity(0.05))
        .cornerRadius(8)
    }

    func stepIcon(_ step: StepRunResult) -> String {
        if step.skipped { return "forward.fill" }
        if step.success { return "checkmark" }
        return "xmark"
    }

    func stepColor(_ step: StepRunResult) -> Color {
        if step.skipped { return .secondary }
        if step.success { return .green }
        return .red
    }
}
