import Testing
@testable import TillerCore

@Test func parsesClaudeAuthStatusJSON() {
    let output = """
    {
      "loggedIn": true,
      "authMethod": "claude.ai",
      "apiProvider": "firstParty",
      "email": "e.palmisano@reply.it",
      "orgId": "3f3bf2c0-7fc0-4cc7-91ad-b729d8902db2",
      "orgName": "e.palmisano@reply.it's Organization",
      "subscriptionType": "max"
    }
    """
    let identity = AgentAccountIdentity.parseClaudeAuthStatus(output)
    #expect(identity?.label == "e.palmisano@reply.it")
    #expect(identity?.orgName == "e.palmisano@reply.it's Organization")
}

@Test func claudeAuthStatusLoggedOutReturnsNil() {
    let output = #"{"loggedIn": false}"#
    #expect(AgentAccountIdentity.parseClaudeAuthStatus(output) == nil)
}

@Test func claudeAuthStatusMalformedReturnsNil() {
    #expect(AgentAccountIdentity.parseClaudeAuthStatus("not json at all") == nil)
}

@Test func parsesCodexLoginStatusPlainLine() {
    let output = "Logged in using an API key - sk-proj-***Y61sA\n"
    #expect(AgentAccountIdentity.parseCodexLoginStatus(output) == "Logged in using an API key - sk-proj-***Y61sA")
}

@Test func codexLoginStatusEmptyReturnsNil() {
    #expect(AgentAccountIdentity.parseCodexLoginStatus("   \n  ") == nil)
}

@Test func codexLoginStatusTakesFirstNonEmptyLine() {
    let output = "\n\nLogged in using ChatGPT - user@example.com\nextra trailing line\n"
    #expect(AgentAccountIdentity.parseCodexLoginStatus(output) == "Logged in using ChatGPT - user@example.com")
}
