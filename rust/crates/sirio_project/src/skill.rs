/// A platform-neutral command description for installing Sirio's agent
/// skill. The caller decides whether to run it in a terminal or background.
///
/// `program` is the bare name (`npx`): resolving it against PATH — on
/// Windows to the `npx.cmd` shim `CreateProcess` cannot infer on its own —
/// is the caller's job, since only the caller knows which environment the
/// command will be spawned in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillInstallCommand {
    pub program: String,
    pub args: Vec<String>,
}

/// The user-level (`-g`) install of Sirio's skill for every agent the
/// skills CLI knows. User-level on purpose: `prepare()` already provisions a
/// machine-managed copy per worktree, so the Settings button's job is the
/// agents the user runs outside a Sirio-launched pane.
///
/// One `-a` per agent: the skills CLI reads a comma-joined value as a
/// single (unknown) agent name and refuses the whole install.
pub fn agent_skill_install_command() -> SkillInstallCommand {
    let mut args: Vec<String> = ["skills", "add", "ai-sirio/sirio", "--skill", "sirio"]
        .into_iter()
        .map(String::from)
        .collect();
    for agent in ["claude-code", "codex", "opencode", "pi"] {
        args.push("-a".into());
        args.push(agent.into());
    }
    args.push("-g".into());
    args.push("-y".into());
    SkillInstallCommand {
        program: "npx".into(),
        args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Settings button installs the skill at the user level (`-g`):
    /// without it the skills CLI installs into the cwd as a project-level
    /// symlink, which both needs symlink privileges on Windows and collides
    /// with the machine-managed `SKILL.md` every `prepare()` already writes
    /// per worktree. Agents are one `-a` each: the comma-joined list the
    /// Swift app passed is rejected by today's skills CLI as a single
    /// unknown agent (`Invalid agents: claude-code,codex,opencode,pi`,
    /// verified live 2026-09-06), which is why Install Skill did nothing.
    #[test]
    fn exposes_the_exact_user_level_install_command() {
        let command = agent_skill_install_command();
        assert_eq!(command.program, "npx");
        assert_eq!(
            command.args,
            vec![
                "skills",
                "add",
                "ai-sirio/sirio",
                "--skill",
                "sirio",
                "-a",
                "claude-code",
                "-a",
                "codex",
                "-a",
                "opencode",
                "-a",
                "pi",
                "-g",
                "-y"
            ]
        );
    }
}
