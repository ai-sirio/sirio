/// Pure keyboard-selection logic for the slash-command popup. The view owns
/// the state; this maps (key, current index, candidate count) to an effect.
enum SlashCommandSelection {
    enum Effect: Equatable {
        case moved(Int)
        case accepted(Int)
        case dismissed
    }

    static func effect(for key: SlashKey, index: Int, count: Int) -> Effect? {
        guard count > 0 else { return nil }
        let clamped = min(max(index, 0), count - 1)
        switch key {
        case .up: return .moved(max(clamped - 1, 0))
        case .down: return .moved(min(clamped + 1, count - 1))
        case .tab, .enter: return .accepted(clamped)
        case .escape: return .dismissed
        }
    }
}
