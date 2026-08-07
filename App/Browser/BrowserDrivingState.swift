import Observation

@MainActor
@Observable
final class BrowserDrivingState {
    var isAgentDriving = false
}
