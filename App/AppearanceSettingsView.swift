import SwiftUI

/// Placeholder — theme/color settings arrive once there is something to
/// configure (see Phase 2A spec Roadmap).
struct AppearanceSettingsView: View {
    var body: some View {
        ContentUnavailableView(
            "Appearance",
            systemImage: "paintbrush",
            description: Text("Theme options coming soon.")
        )
    }
}
