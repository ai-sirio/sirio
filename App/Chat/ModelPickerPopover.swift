import SwiftUI
import TillerACP

/// Model + effort picker: search field, driver-supplied model list with the
/// driver's first entry badged as recommended, effort chips at the bottom.
struct ModelPickerPopover: View {
    let controller: ChatController
    @Binding var isPresented: Bool
    @State private var query = ""

    private var models: [ModelInfo] {
        controller.models?.availableModels ?? []
    }
    private var filtered: [ModelInfo] {
        ModelPickerFilter.filter(models, query: query)
    }
    private var recommendedId: String? { models.first?.modelId }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("Search models…", text: $query)
                .textFieldStyle(.roundedBorder)
                .controlSize(.small)
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(filtered, id: \.modelId) { model in
                        modelRow(model)
                    }
                    if filtered.isEmpty {
                        Text("No models match")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .padding(6)
                    }
                }
            }
            .frame(maxHeight: 260)
            effortSection
        }
        .padding(10)
        .frame(width: 300)
    }

    private func modelRow(_ model: ModelInfo) -> some View {
        Button {
            isPresented = false
            Task { await controller.setModel(model.modelId) }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "checkmark")
                    .font(.caption2.weight(.semibold))
                    .opacity(model.modelId == controller.models?.currentModelId ? 1 : 0)
                VStack(alignment: .leading, spacing: 1) {
                    Text(model.name).font(.callout)
                    if let description = model.description, !description.isEmpty {
                        Text(description)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
                Spacer(minLength: 0)
                if model.modelId == recommendedId {
                    Text("Recommended")
                        .font(.caption2)
                        .padding(.horizontal, 5).padding(.vertical, 1)
                        .background(Color.accentColor.opacity(0.15), in: Capsule())
                        .foregroundStyle(Color.accentColor)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.vertical, 3).padding(.horizontal, 4)
        .background(model.modelId == controller.models?.currentModelId
                        ? AnyShapeStyle(.quaternary.opacity(0.5))
                        : AnyShapeStyle(Color.clear),
                    in: RoundedRectangle(cornerRadius: 6))
    }

    @ViewBuilder
    private var effortSection: some View {
        if let effort = controller.effortOption,
           let choices = effort.options, !choices.isEmpty {
            Divider()
            HStack(spacing: 6) {
                Text(effort.name ?? "Effort")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                ForEach(choices, id: \.value) { choice in
                    Button(choice.name) {
                        Task { await controller.setEffort(choice.value) }
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.mini)
                    .tint(choice.value == effort.currentValue
                              ? Color.accentColor : Color.secondary)
                }
            }
        }
    }
}
