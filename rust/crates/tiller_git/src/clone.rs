//! Git clone with live transfer progress.

use std::path::Path;

use crate::GitError;
use crate::git::GitRunner;

/// Namespace for clone operations.
pub struct GitClone;

impl GitClone {
    /// Clones `url` into `destination`, forwarding each `Receiving objects`
    /// percentage as a value from 0.0 through 1.0. The streaming runner
    /// owns process completion and reports git failures through `GitError`.
    pub fn clone<F>(url: &str, destination: &Path, mut on_progress: F) -> Result<(), GitError>
    where
        F: FnMut(f64),
    {
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut arguments = vec!["clone".to_string(), "--progress".to_string()];
        // Git's local-clone optimization can avoid the receiving phase
        // entirely. Disabling it for a local source keeps progress behavior
        // observable while remaining offline and fast for local fixtures.
        if Path::new(url).exists() || parent.join(url).exists() {
            arguments.push("--no-local".to_string());
        }
        arguments.push(url.to_string());
        arguments.push(destination.to_string_lossy().into_owned());
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        GitRunner::run_streaming(&arguments, parent, move |line| {
            if let Some(progress) = Self::parse_progress(&line) {
                on_progress(progress);
            }
        })?;
        Ok(())
    }

    /// Extracts only `Receiving objects: N%` progress. Other clone phases do
    /// not describe object transfer and are intentionally ignored.
    pub fn parse_progress(line: &str) -> Option<f64> {
        let line = line.trim_start();
        let rest = line.strip_prefix("Receiving objects:")?;
        let percent_end = rest.find('%')?;
        let digits = rest[..percent_end].trim();
        let percent = digits.parse::<f64>().ok()?;
        if !(0.0..=100.0).contains(&percent) {
            return None;
        }
        Some(percent / 100.0)
    }
}

/// Convenience free-function spelling of [`GitClone::clone`].
pub fn clone_repository<F>(url: &str, destination: &Path, on_progress: F) -> Result<(), GitError>
where
    F: FnMut(f64),
{
    GitClone::clone(url, destination, on_progress)
}
