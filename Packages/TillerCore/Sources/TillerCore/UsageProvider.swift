/// The AI providers whose usage quota Tiller polls and shows in the usage bar.
///
/// `CaseIterable` is what keeps the polling timer honest: the store derives
/// "is anything enabled" by folding over every case, so a new provider can't
/// be added without its toggle taking part in the timer lifecycle.
public enum UsageProvider: String, CaseIterable, Sendable, Hashable {
    case claude
    case codex
    case opencodeGo
    case ollamaCloud

    /// UserDefaults key for this provider's "show in usage bar" toggle,
    /// which doubles as its polling switch.
    public var showInBarKey: String { "usage.\(rawValue).showInBar" }
}
