import SwiftUI

struct AddGroupView: View {
    @EnvironmentObject var viewModel: AppViewModel
    @Environment(\.dismiss) private var dismiss

    @State private var name = ""
    @State private var description = ""
    @State private var color = ""

    var body: some View {
        VStack(spacing: 0) {
            Text("New Group")
                .font(.headline)
                .padding()

            Form {
                TextField("Name", text: $name)
                TextField("Description (optional)", text: $description)
                Picker("Color", selection: $color) {
                    Text("None").tag("")
                    Text("Red").tag("red")
                    Text("Blue").tag("blue")
                    Text("Green").tag("green")
                    Text("Orange").tag("orange")
                    Text("Purple").tag("purple")
                }
            }
            .padding()

            HStack {
                Button("Cancel") {
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)

                Spacer()

                Button("Create") {
                    Task {
                        await viewModel.createGroup(
                            name: name,
                            description: description.isEmpty ? nil : description,
                            color: color.isEmpty ? nil : color
                        )
                        dismiss()
                    }
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
                .disabled(name.isEmpty)
            }
            .padding()
        }
        .frame(width: 400, height: 280)
    }
}
