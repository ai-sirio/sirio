import Foundation

public enum AppVersion {
    /// Versione marketing dal bundle dell'app (CFBundleShortVersionString di
    /// project.yml). Nei test il bundle è quello del runner: usare
    /// version(fromInfo:) per la logica.
    public static var current: String {
        version(fromInfo: Bundle.main.infoDictionary)
    }

    public static func version(fromInfo info: [String: Any]?) -> String {
        info?["CFBundleShortVersionString"] as? String ?? "0.0.0"
    }
}
