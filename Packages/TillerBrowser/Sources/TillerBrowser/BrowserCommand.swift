public enum BrowserCommand: Equatable, Sendable {
    public enum Navigation: String, Equatable, Sendable {
        case back
        case forward
        case reload
    }

    public enum Get: String, Equatable, Sendable {
        case url
        case text
        case html
    }

    case snapshot
    case eval(String)

    case open(url: String)
    case navigate(Navigation)
    case get(Get)
}

public typealias BrowserNavigation = BrowserCommand.Navigation
public typealias BrowserGet = BrowserCommand.Get
