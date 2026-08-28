/// A platform-neutral command description for installing Sirio's agent
/// skill. The caller decides whether to run it in a terminal or background.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillInstallCommand {
    pub program: String,
    pub args: Vec<String>,
}

pub fn agent_skill_install_command() -> SkillInstallCommand {
    SkillInstallCommand {
        program: "npx".into(),
        args: vec![
            "skills".into(),
            "add".into(),
            "e-palmisano/sirio".into(),
            "--skill".into(),
            "sirio".into(),
            "-a".into(),
            "claude-code,codex,opencode,pi".into(),
            "-y".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_the_exact_non_gui_install_command() {
        let command = agent_skill_install_command();
        assert_eq!(command.program, "npx");
        assert_eq!(
            command.args,
            vec![
                "skills",
                "add",
                "e-palmisano/sirio",
                "--skill",
                "sirio",
                "-a",
                "claude-code,codex,opencode,pi",
                "-y"
            ]
        );
    }
}
