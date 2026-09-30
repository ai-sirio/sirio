//! What a token says it may do, and whether that includes writing
//! (spec §9): Settings → Git Hosting names the scope to *read* apart from the
//! scope to *act*, and says which one the saved token has when the forge
//! reports it.

use crate::model::Forge;

/// The scopes a token reports for itself. GitHub sends them in the
/// `X-OAuth-Scopes` header of a classic token — a fine-grained token sends
/// none, and is `None` at the call site — and GitLab in a personal access
/// token's own record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenScopes(pub Vec<String>);

impl TokenScopes {
    /// `repo, read:org` → `["repo", "read:org"]`: split on commas, trimmed,
    /// empty entries dropped. An empty header is a token with no scopes.
    pub fn from_header(header: &str) -> Self {
        Self(
            header
                .split(',')
                .map(str::trim)
                .filter(|scope| !scope.is_empty())
                .map(str::to_string)
                .collect(),
        )
    }

    /// Whether a token with these scopes may comment, review and edit:
    /// GitHub's `repo` (or `public_repo`, for public repositories), GitLab's
    /// `api`. `read_api`, `read:org` and the rest only read.
    pub fn allows_writing(&self, forge: Forge) -> bool {
        self.0.iter().any(|scope| match forge {
            Forge::GitHub => matches!(scope.as_str(), "repo" | "public_repo"),
            Forge::GitLab => scope == "api",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_is_split_trimmed_and_stripped_of_blanks() {
        assert_eq!(TokenScopes::from_header("repo, read:org").0, ["repo", "read:org"]);
        assert_eq!(TokenScopes::from_header("  repo  ,, read:org ,").0, ["repo", "read:org"]);
        assert!(TokenScopes::from_header("").0.is_empty());
        assert!(TokenScopes::from_header(" , ").0.is_empty());
    }

    #[test]
    fn scopes_that_only_read_do_not_allow_writing() {
        for scopes in ["read:org", "read:user, user:email", "", "notifications"] {
            assert!(!TokenScopes::from_header(scopes).allows_writing(Forge::GitHub), "{scopes:?}");
        }
        for scopes in ["read_api", "read_api, read_repository", "read_user", ""] {
            assert!(!TokenScopes::from_header(scopes).allows_writing(Forge::GitLab), "{scopes:?}");
        }
    }

    #[test]
    fn the_write_scope_of_each_forge_allows_writing_and_not_the_others() {
        assert!(TokenScopes::from_header("repo, read:org").allows_writing(Forge::GitHub));
        assert!(TokenScopes::from_header("public_repo").allows_writing(Forge::GitHub));
        assert!(TokenScopes::from_header("read_api, api").allows_writing(Forge::GitLab));
        assert!(!TokenScopes::from_header("api").allows_writing(Forge::GitHub), "a GitLab scope means nothing on GitHub");
        assert!(!TokenScopes::from_header("repo").allows_writing(Forge::GitLab));
    }

    #[test]
    fn a_scope_is_matched_whole_not_by_prefix() {
        assert!(!TokenScopes::from_header("repository, api_read").allows_writing(Forge::GitHub));
        assert!(!TokenScopes::from_header("api_read, apis").allows_writing(Forge::GitLab));
    }
}
