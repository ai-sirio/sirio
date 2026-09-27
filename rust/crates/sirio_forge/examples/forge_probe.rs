//! Drives `sirio_forge` the way the app does, for
//! `Scripts/Tests/test-forge-e2e.sh`.
//!
//! One command per run. The answer is printed on stdout as `KEY value`
//! lines, one fact per line, so the script can match each with `grep -qxF`.
//! A `ForgeError` prints `ERR <Variant>` (plus `SSO <url>` or `RESET <unix>`
//! when the error carries one) and exits 20; bad usage exits 2. The script
//! decides which outcome is expected.
//!
//! Not a `#[test]`: it needs forges on loopback, the real `gh` and `glab`,
//! and the debug-only `SIRIO_FORGE_TEST_ENDPOINTS` door.
//!
//! ```text
//! forge_probe --forge github|gitlab --host H --project P (--cli | --token T) <command> [args]
//! ```

use std::process::ExitCode;

use sirio_forge::{
    CliProgram, CliTransport, Forge, ForgeClient, ForgeError, ForgeTarget, HostSetting, Means,
    Resolution, SystemProbes, TokenTransport, Transport, resolve,
};

enum Failure {
    Usage(String),
    Forge(ForgeError),
}

impl From<ForgeError> for Failure {
    fn from(error: ForgeError) -> Self {
        Self::Forge(error)
    }
}

fn usage(message: &str) -> Failure {
    Failure::Usage(message.to_string())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&Args::parse(&args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("usage: {message}");
            ExitCode::from(2)
        }
        Err(Failure::Forge(error)) => {
            println!("ERR {}", error.variant_name());
            match &error {
                ForgeError::Forbidden {
                    sso_url: Some(url), ..
                } => println!("SSO {url}"),
                ForgeError::RateLimited {
                    reset_at: Some(reset),
                    ..
                } => println!("RESET {reset}"),
                _ => {}
            }
            ExitCode::from(20)
        }
    }
}

/// `--name value` flags, `--cli`/`--more` switches, and the bare words in
/// order.
struct Args {
    flags: Vec<(String, String)>,
    switches: Vec<String>,
    words: Vec<String>,
}

impl Args {
    const SWITCHES: [&'static str; 2] = ["cli", "more"];

    fn parse(raw: &[String]) -> Self {
        let mut args = Self {
            flags: Vec::new(),
            switches: Vec::new(),
            words: Vec::new(),
        };
        let mut iter = raw.iter();
        while let Some(item) = iter.next() {
            match item.strip_prefix("--") {
                Some(name) if Self::SWITCHES.contains(&name) => {
                    args.switches.push(name.to_string())
                }
                Some(name) => {
                    let value = iter.next().cloned().unwrap_or_default();
                    args.flags.push((name.to_string(), value));
                }
                None => args.words.push(item.clone()),
            }
        }
        args
    }

    fn flag(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    fn switch(&self, name: &str) -> bool {
        self.switches.iter().any(|switch| switch == name)
    }
}

fn forge_flag(args: &Args, name: &str) -> Result<Forge, Failure> {
    match args.flag(name) {
        Some("github") => Ok(Forge::GitHub),
        Some("gitlab") => Ok(Forge::GitLab),
        _ => Err(usage(&format!("--{name} github|gitlab"))),
    }
}

fn client(args: &Args) -> Result<ForgeClient, Failure> {
    let forge = forge_flag(args, "forge")?;
    let host = args
        .flag("host")
        .ok_or_else(|| usage("--host H"))?
        .to_string();
    let project = args
        .flag("project")
        .ok_or_else(|| usage("--project P"))?
        .to_string();
    let transport: Box<dyn Transport> = if args.switch("cli") {
        Box::new(CliTransport::new(CliProgram::for_forge(forge), &host))
    } else {
        let token = args
            .flag("token")
            .ok_or_else(|| usage("--cli or --token T"))?;
        Box::new(TokenTransport::new(forge, &host, token.to_string()))
    };
    Ok(ForgeClient::new(
        forge,
        ForgeTarget { host, project },
        transport,
    ))
}

fn run(args: &Args) -> Result<(), Failure> {
    let command = args
        .words
        .first()
        .cloned()
        .ok_or_else(|| usage("a command"))?;
    if command == "resolve" {
        return resolve_command(args);
    }
    let client = client(args)?;
    match command.as_str() {
        "viewer" => println!("VIEWER {}", client.viewer()?),
        other => return Err(usage(&format!("unknown command {other}"))),
    }
    Ok(())
}

fn forge_word(forge: Forge) -> &'static str {
    match forge {
        Forge::GitHub => "github",
        Forge::GitLab => "gitlab",
    }
}

/// `resolve --host H [--setting github|gitlab[:cli|token]] [--token-forge github|gitlab]`
fn resolve_command(args: &Args) -> Result<(), Failure> {
    let host = args.flag("host").ok_or_else(|| usage("--host H"))?;
    let setting = match args.flag("setting") {
        None => None,
        Some(text) => {
            let (forge, means) = match text.split_once(':') {
                Some((forge, means)) => (forge, Some(means)),
                None => (text, None),
            };
            let forge = match forge {
                "github" => Forge::GitHub,
                "gitlab" => Forge::GitLab,
                _ => return Err(usage("--setting github|gitlab[:cli|token]")),
            };
            let means = match means {
                None => None,
                Some("cli") => Some(Means::Cli),
                Some("token") => Some(Means::Token),
                Some(_) => return Err(usage("--setting github|gitlab[:cli|token]")),
            };
            Some(HostSetting {
                host: host.to_string(),
                forge,
                means,
            })
        }
    };
    let stored = match args.flag("token-forge") {
        None => None,
        Some(_) => Some(forge_flag(args, "token-forge")?),
    };
    match resolve(host, setting.as_ref(), stored, &SystemProbes) {
        Resolution::Ready { forge, means } => {
            let means = match means {
                Means::Cli => "cli",
                Means::Token => "token",
            };
            println!("RESOLVE ready {} {means}", forge_word(forge));
        }
        Resolution::NotConnected { forge } => {
            println!("RESOLVE not-connected {}", forge_word(forge))
        }
        Resolution::UnknownForge => println!("RESOLVE unknown"),
    }
    Ok(())
}
