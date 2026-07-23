import Foundation

/// Search filter for the model picker popover. Pure, order-preserving.
public enum ModelPickerFilter {
    public static func filter(_ models: [ModelInfo], query: String) -> [ModelInfo] {
        let trimmed = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !trimmed.isEmpty else { return models }
        return models.filter { model in
            model.name.lowercased().contains(trimmed)
                || model.modelId.lowercased().contains(trimmed)
                || (model.description?.lowercased().contains(trimmed) ?? false)
        }
    }
}
