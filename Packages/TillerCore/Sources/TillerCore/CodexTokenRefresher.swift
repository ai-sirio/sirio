import Foundation

public enum CodexTokenRefreshError: Error, Equatable {
    case expired
    case revoked
    case reused
    case network
    case invalidResponse
}

public enum CodexTokenRefresher {
    private static let refreshURL = URL(string: "https://auth.openai.com/oauth/token")!
    private static let clientId = "app_EMoamEEZ73f0CkXaXp7hrann"

    public static func refresh(
        _ credentials: CodexOAuthCredentials,
        transport: any CodexHTTPTransport
    ) async throws -> CodexOAuthCredentials {
        var request = URLRequest(url: refreshURL)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try? JSONSerialization.data(withJSONObject: [
            "client_id": clientId,
            "grant_type": "refresh_token",
            "refresh_token": credentials.refreshToken,
            "scope": "openid profile email"
        ])

        let data: Data
        let response: HTTPURLResponse
        do {
            (data, response) = try await transport.send(request)
        } catch {
            throw CodexTokenRefreshError.network
        }

        if response.statusCode == 401 {
            throw classify401(data)
        }
        guard
            response.statusCode == 200,
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let accessToken = json["access_token"] as? String
        else {
            throw CodexTokenRefreshError.invalidResponse
        }

        return CodexOAuthCredentials(
            accessToken: accessToken,
            refreshToken: json["refresh_token"] as? String ?? credentials.refreshToken,
            idToken: json["id_token"] as? String ?? credentials.idToken,
            accountId: credentials.accountId,
            lastRefresh: Date()
        )
    }

    private static func classify401(_ data: Data) -> CodexTokenRefreshError {
        guard
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let code = (json["error"] as? [String: Any])?["code"] as? String
                ?? json["error"] as? String
        else {
            return .expired
        }
        switch code.lowercased() {
        case "refresh_token_reused": return .reused
        case "refresh_token_invalidated": return .revoked
        default: return .expired
        }
    }
}
