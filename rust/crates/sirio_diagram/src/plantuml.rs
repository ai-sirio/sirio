//! PlantUML through the `plantuml` on PATH (design §3). Found, never
//! shipped — `sirio_lsp`'s model for language servers.

use std::ffi::OsStr;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{DiagramError, Options, Svg, plantuml_document, svg};

/// How long a local render may take: a cold JVM alone takes 1–3 s.
pub(crate) const LOCAL_TIMEOUT: Duration = Duration::from_secs(15);

/// Finds `program` on `search_path` the way a shell would. On Windows the
/// `PATHEXT` extensions are tried in order, because package managers install
/// PlantUML as a `.cmd` or `.bat` shim that `Command::new("plantuml")` alone
/// would not find.
pub(crate) fn find_program(program: &str, search_path: &OsStr) -> Option<PathBuf> {
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(|extension| extension.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(search_path) {
        for extension in &extensions {
            let candidate = dir.join(format!("{program}{extension}"));
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Renders with the local `plantuml`, source on stdin and SVG on stdout,
/// from the Markdown file's directory.
///
/// Sandboxed (design §3): the `ALLOWLIST` security profile blocks URLs and
/// environment variables, and `plantuml.allowlist.path` admits files only
/// under the worktree. PlantUML reads both from the environment as well as
/// from Java system properties, so no JVM flag has to reach the launcher.
pub(crate) fn render_local(
    source: &str,
    options: &Options,
    search_path: &OsStr,
    timeout: Duration,
) -> Result<Svg, DiagramError> {
    let Some(program) = find_program("plantuml", search_path) else {
        return Err(DiagramError::NotAvailable);
    };
    let input = plantuml_document(source).into_owned().into_bytes();
    let mut child = Command::new(&program)
        .args(["-tsvg", "-pipe", "-charset", "UTF-8"])
        .current_dir(&options.working_dir)
        // The launcher script finds `java` on the same PATH it was found on.
        .env("PATH", search_path)
        .env("PLANTUML_SECURITY_PROFILE", "ALLOWLIST")
        .env("plantuml.allowlist.path", &options.include_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => DiagramError::NotAvailable,
            _ => DiagramError::Io(error.to_string()),
        })?;

    // Writer and readers on threads of their own: a diagram big enough to
    // fill a pipe would otherwise deadlock against a child blocked writing.
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let error_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(DiagramError::Timeout {
                    seconds: timeout.as_secs(),
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(DiagramError::Io(error.to_string())),
        }
    };
    let _ = writer.join();
    let stdout = reader.join().unwrap_or_default();
    let stderr = error_reader.join().unwrap_or_default();

    if !status.success() || has_error_marker(&stderr) {
        return Err(syntax_error(&stderr));
    }
    svg::double_for_hidpi(&String::from_utf8_lossy(&stdout))
        .ok_or_else(|| DiagramError::Io("plantuml printed no SVG".into()))
}

fn has_error_marker(stderr: &str) -> bool {
    stderr.lines().any(|line| line.trim() == "ERROR")
}

/// `plantuml -pipe` reports a bad diagram as `ERROR`, the line number, then
/// the message, on stderr.
fn syntax_error(stderr: &str) -> DiagramError {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if let Some(at) = lines.iter().position(|line| *line == "ERROR") {
        let line = lines.get(at + 1).and_then(|line| line.parse::<u32>().ok());
        let skip = if line.is_some() { 2 } else { 1 };
        let message = lines
            .get(at + skip..)
            .map(|rest| rest.join(" "))
            .unwrap_or_default();
        return DiagramError::Syntax {
            message: if message.is_empty() {
                "syntax error".into()
            } else {
                message
            },
            line,
        };
    }
    DiagramError::Syntax {
        message: lines
            .last()
            .map_or_else(|| "plantuml failed".into(), |line| line.to_string()),
        line: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagramKind, render, scratch_dir, test_options};
    use std::net::TcpListener;
    use std::time::Instant;

    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"20\" viewBox=\"0 0 40 20\"></svg>";

    /// `bin` first, then the real PATH: the fake scripts call `cat`, `env`
    /// and `sleep`, and `render_local` hands the child this same PATH.
    #[cfg(unix)]
    fn path_with(bin: &Path) -> std::ffi::OsString {
        let rest = std::env::var_os("PATH").unwrap_or_default();
        std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&rest)))
            .expect("joinable PATH")
    }

    #[cfg(unix)]
    fn fake_plantuml(dir: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).expect("bin dir");
        let path = dir.join("plantuml");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fake plantuml");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    #[test]
    fn nothing_on_the_path_is_not_available() {
        let root = scratch_dir("plantuml-none");
        let empty = root.join("bin");
        std::fs::create_dir_all(&empty).expect("bin dir");
        assert_eq!(
            render_local(
                "A -> B",
                &test_options(&root),
                empty.as_os_str(),
                LOCAL_TIMEOUT
            ),
            Err(DiagramError::NotAvailable)
        );
    }

    #[cfg(unix)]
    #[test]
    fn its_svg_is_the_diagram() {
        let root = scratch_dir("plantuml-svg");
        let bin = root.join("bin");
        fake_plantuml(&bin, &format!("cat > /dev/null\nprintf '%s' '{SVG}'"));
        let svg = render_local(
            "A -> B",
            &test_options(&root),
            &path_with(&bin),
            LOCAL_TIMEOUT,
        )
        .expect("the fake prints an SVG");
        assert_eq!((svg.logical_width, svg.logical_height), (40, 20));
    }

    #[cfg(unix)]
    #[test]
    fn it_receives_the_wrapped_source_in_the_files_directory_sandboxed() {
        let root = scratch_dir("plantuml-capture");
        let bin = root.join("bin");
        let docs = root.join("docs");
        let out = root.join("out");
        std::fs::create_dir_all(&docs).expect("docs");
        std::fs::create_dir_all(&out).expect("out");
        let out_dir = out.display();
        fake_plantuml(
            &bin,
            &format!(
                "cat > '{out_dir}/stdin'\nenv > '{out_dir}/env'\npwd > '{out_dir}/pwd'\nprintf '%s' '{SVG}'"
            ),
        );
        let options = Options {
            working_dir: docs.clone(),
            include_root: root.clone(),
            ..test_options(&root)
        };
        render_local("A -> B", &options, &path_with(&bin), LOCAL_TIMEOUT).expect("renders");
        assert_eq!(
            std::fs::read_to_string(out.join("stdin")).expect("stdin"),
            "@startuml\nA -> B\n@enduml\n"
        );
        let env = std::fs::read_to_string(out.join("env")).expect("env");
        assert!(
            env.lines()
                .any(|line| line == "PLANTUML_SECURITY_PROFILE=ALLOWLIST")
        );
        assert!(
            env.lines()
                .any(|line| line == format!("plantuml.allowlist.path={}", root.display()))
        );
        let pwd = std::fs::read_to_string(out.join("pwd")).expect("pwd");
        assert_eq!(
            std::fs::canonicalize(pwd.trim()).expect("pwd exists"),
            std::fs::canonicalize(&docs).expect("docs exists")
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_error_exit_is_a_syntax_error_at_its_line() {
        let root = scratch_dir("plantuml-error");
        let bin = root.join("bin");
        fake_plantuml(
            &bin,
            "cat > /dev/null\nprintf 'ERROR\\n3\\nSyntax Error?\\n' >&2\nexit 200",
        );
        assert_eq!(
            render_local(
                "A -> ",
                &test_options(&root),
                &path_with(&bin),
                LOCAL_TIMEOUT
            ),
            Err(DiagramError::Syntax {
                message: "Syntax Error?".into(),
                line: Some(3)
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_error_marker_counts_even_when_the_exit_is_zero() {
        let root = scratch_dir("plantuml-error-zero");
        let bin = root.join("bin");
        fake_plantuml(
            &bin,
            &format!(
                "cat > /dev/null\nprintf 'ERROR\\n1\\nNo diagram found\\n' >&2\nprintf '%s' '{SVG}'"
            ),
        );
        assert_eq!(
            render_local("x", &test_options(&root), &path_with(&bin), LOCAL_TIMEOUT),
            Err(DiagramError::Syntax {
                message: "No diagram found".into(),
                line: Some(1)
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_plantuml_is_killed_at_the_timeout() {
        let root = scratch_dir("plantuml-hang");
        let bin = root.join("bin");
        fake_plantuml(&bin, "exec sleep 5");
        let started = Instant::now();
        let result = render_local(
            "A -> B",
            &test_options(&root),
            &path_with(&bin),
            Duration::from_millis(300),
        );
        assert!(
            matches!(result, Err(DiagramError::Timeout { .. })),
            "got {result:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the child was not waited out"
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_finds_a_cmd_shim_through_pathext() {
        let dir = scratch_dir("plantuml-pathext");
        std::fs::write(dir.join("plantuml.cmd"), "@echo off\r\n").expect("shim");
        assert_eq!(
            find_program("plantuml", dir.as_os_str()),
            Some(dir.join("plantuml.cmd"))
        );
    }

    fn real_plantuml() -> bool {
        let found =
            find_program("plantuml", &std::env::var_os("PATH").unwrap_or_default()).is_some();
        if !found {
            eprintln!("SKIP: plantuml is not on PATH");
        }
        found
    }

    #[test]
    fn real_plantuml_renders_a_sequence_diagram() {
        if !real_plantuml() {
            return;
        }
        let root = scratch_dir("plantuml-real");
        let svg = render(
            DiagramKind::PlantUml,
            "Alice -> Bob: hello",
            &test_options(&root),
        )
        .expect("plantuml renders");
        assert!(svg.markup.contains("Alice"));
        assert!(svg.logical_width > 0);
    }

    /// The design's sandbox requirement (§3): includes only inside the
    /// worktree, and no network at all.
    #[test]
    fn real_plantuml_is_sandboxed_to_the_worktree_and_offline() {
        if !real_plantuml() {
            return;
        }
        let root = scratch_dir("plantuml-sandbox");
        let docs = root.join("docs");
        std::fs::create_dir_all(&docs).expect("docs");
        std::fs::write(root.join("inside.puml"), "Alice -> Bob: INSIDE\n").expect("inside");
        let outside = scratch_dir("plantuml-outside");
        std::fs::write(outside.join("secret.puml"), "Alice -> Bob: SECRET\n").expect("secret");
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("addr").port();
        let options = Options {
            working_dir: docs,
            include_root: root.clone(),
            ..test_options(&root)
        };

        let inside = render(
            DiagramKind::PlantUml,
            &format!("!include {}\n", root.join("inside.puml").display()),
            &options,
        )
        .expect("an include inside the worktree renders");
        assert!(inside.markup.contains("INSIDE"));

        let refused = render(
            DiagramKind::PlantUml,
            &format!(
                "!include {}\n!includeurl http://127.0.0.1:{port}/x.puml\nA -> B\n",
                outside.join("secret.puml").display()
            ),
            &options,
        );
        if let Ok(svg) = &refused {
            assert!(
                !svg.markup.contains("SECRET"),
                "a file outside the worktree reached the diagram"
            );
        }
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "PlantUML connected to the loopback listener"
        );
    }
}
