/// State machine pura del ciclo di auto-update. Le transizioni non valide
/// per lo stato corrente ritornano lo stato invariato: i callback di Sparkle
/// possono arrivare in ordini inattesi e non devono mai corrompere la UI.
public enum UpdateState: Equatable, Sendable {
    case idle
    /// Check esplicito dell'utente in corso; i check di sistema restano silenziosi.
    case checking
    case available(version: String)
    case downloading(version: String, progress: Double)
    case readyToInstall(version: String)
    /// Esito visibile solo dopo un check manuale.
    case upToDate
    case error(message: String)

    public func manualCheckStarted() -> UpdateState {
        switch self {
        case .idle, .upToDate, .error: return .checking
        default: return self
        }
    }

    public func found(version: String) -> UpdateState {
        switch self {
        case .idle, .checking, .upToDate, .error, .available:
            return .available(version: version)
        default:
            return self
        }
    }

    public func notFound() -> UpdateState {
        switch self {
        case .checking: return .upToDate
        case .idle, .upToDate: return .idle
        default: return self
        }
    }

    public func downloadStarted() -> UpdateState {
        guard case .available(let version) = self else { return self }
        return .downloading(version: version, progress: 0)
    }

    public func downloadProgressed(_ fraction: Double) -> UpdateState {
        guard case .downloading(let version, let progress) = self else { return self }
        let clamped = min(1, max(0, fraction))
        return .downloading(version: version, progress: max(progress, clamped))
    }

    public func downloadCompleted() -> UpdateState {
        switch self {
        case .downloading(let version, _), .available(let version):
            return .readyToInstall(version: version)
        default:
            return self
        }
    }

    public func failed(message: String) -> UpdateState {
        .error(message: message)
    }

    public func dismissed() -> UpdateState {
        .idle
    }
}
