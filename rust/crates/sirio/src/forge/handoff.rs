//! Handing a change request's worktree to an agent (change requests C2, spec
//! C §9): read the context the purpose needs, check the worktree out the way
//! C1 does, then write the context file the agent is told to read. Runs off
//! the GPUI thread. A failed read touches no git; a failed write leaves the
//! worktree in place.

use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sirio_forge::{Forge, ForgeError, Purpose, Scope};
use sirio_ui::ansi_log::LogFlavor;
use sirio_ui::change_request_tab::HandoffPreview;
use sirio_ui::forge_source::ChangeRequestSource;
use sirio_ui::handoff::context::{self, HANDOFF_DIR, RenderInput};

use super::ForgeHub;
use super::checkout::{CheckoutDone, CheckoutRequest, WorktreeState, name_of};

/// What the dialog asked for: the purpose, its narrowing, and the user's own words.
pub(crate) struct HandoffAsk {
    pub(crate) purpose: Purpose,
    pub(crate) scope: Scope,
    pub(crate) instructions: String,
}

pub(crate) struct HandoffDone {
    pub(crate) checkout: CheckoutDone,
    /// `.sirio/handoff/<name>`, relative to the worktree.
    pub(crate) relative: String,
    /// The two-line prompt the agent is started with.
    pub(crate) prompt: String,
    /// Sirio's note on the worktree, when the file says one (see `worktree_note`).
    pub(crate) note: Option<&'static str>,
}

/// A hand-off that did not finish. `rate_limited_until` is the forge's reset
/// in Unix seconds, set only when the read was refused by a rate limit that
/// named one, so the tab can pause until then.
pub(crate) struct HandoffFailure {
    pub(crate) message: String,
    pub(crate) rate_limited_until: Option<i64>,
}

impl HandoffFailure {
    fn plain(message: String) -> Self {
        Self { message, rate_limited_until: None }
    }
}

/// What a Review hand-off tells the agent about pushing: a review never pushes.
const REVIEW_PUSH: &str = "Do not push: this is a review.";

impl ForgeHub {
    /// The dialog's *Worktree* line and whether the viewer wrote the change
    /// request. A dry plan: it reads the forge and the worktrees, and creates
    /// or removes nothing in them.
    pub(crate) fn handoff_preview(&self, request: &CheckoutRequest) -> HandoffPreview {
        let reference = &request.reference;
        let Ok(client) = self.client_for(reference) else {
            return HandoffPreview::Ready {
                worktree: Err(format!("Sirio is not connected to {}", reference.host)),
                viewer_is_author: None,
            };
        };
        match self.plan(request, true) {
            Ok(planned) => HandoffPreview::Ready {
                worktree: sirio_ui::handoff::describe(&planned.target, &planned.plan, name_of),
                viewer_is_author: client
                    .viewer()
                    .ok()
                    .map(|viewer| viewer.eq_ignore_ascii_case(&planned.header.summary.author)),
            },
            Err(error) => HandoffPreview::Ready { worktree: Err(error), viewer_is_author: None },
        }
    }

    /// Reads the context, checks out, writes the file, excludes the folder
    /// from git and prunes old files. Off the GPUI thread.
    pub(crate) fn handoff(&self, request: &CheckoutRequest, ask: &HandoffAsk) -> Result<HandoffDone, HandoffFailure> {
        let reference = &request.reference;
        let label = reference.label();
        let client = self
            .client_for(reference)
            .map_err(|_| HandoffFailure::plain(format!("Sirio is not connected to {}", reference.host)))?;

        // Read first: a read that fails, rate limit included, touches no git.
        let context = client.context(reference.number, ask.purpose, &ask.scope).map_err(|error| HandoffFailure {
            message: format!("could not read {label}: {error}"),
            rate_limited_until: rate_limit_reset(&error),
        })?;

        let checkout = self.checkout(request).map_err(HandoffFailure::plain)?;
        let note = worktree_note(&checkout, ask.purpose)?;
        let nonce = new_nonce().map_err(|error| {
            HandoffFailure::plain(format!("making the context file's nonce failed: {error}"))
        })?;

        let flavor = match reference.forge {
            Forge::GitHub => LogFlavor::GitHub,
            Forge::GitLab => LogFlavor::GitLab,
        };
        let push: &str = if ask.purpose == Purpose::Review { REVIEW_PUSH } else { &checkout.push };
        let text = context::render(&RenderInput {
            context: &context,
            purpose: ask.purpose,
            scope: &ask.scope,
            label: &label,
            push,
            instructions: &ask.instructions,
            nonce: &nonce,
            worktree_note: note,
            flavor,
        });

        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let name = context::file_name(reference.number, ask.purpose, &stamp);
        let dir = handoff_dir(&checkout.path)
            .map_err(|error| HandoffFailure::plain(format!("writing the context file failed: {error}")))?;
        write_context(&dir.join(&name), &text)
            .map_err(|error| HandoffFailure::plain(format!("writing the context file failed: {error}")))?;
        let relative = format!("{HANDOFF_DIR}/{name}");
        sirio_git::exclude(&request.repo, &format!("{HANDOFF_DIR}/")).map_err(|error| {
            HandoffFailure::plain(format!(
                "keeping the context file out of git failed: {error} (the context file is at {relative})"
            ))
        })?;
        prune(&dir);

        let prompt = context::launch_prompt(&relative, &label);
        Ok(HandoffDone { checkout, relative, prompt, note })
    }
}

/// A new nonce for one file's untrusted blocks: 16 lowercase hex digits from
/// the operating system's random source. A forge cannot know it, so it cannot
/// write a block end that the file counts.
fn new_nonce() -> Result<String, getrandom::Error> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::new_nonce;
    use sirio_ui::handoff::context::is_nonce;

    #[test]
    fn a_nonce_is_sixteen_lowercase_hex_digits_and_differs_per_file() {
        let (first, second) = (new_nonce().unwrap(), new_nonce().unwrap());
        assert!(is_nonce(&first) && is_nonce(&second), "{first} {second}");
        assert_ne!(first, second);
    }
}

/// Sirio's note on the worktree the agent starts in, or the refusal when the
/// agent would work on a branch that is not the change request's. A review
/// reads by commit and never pushes, so another branch is fine for it.
fn worktree_note(checkout: &CheckoutDone, purpose: Purpose) -> Result<Option<&'static str>, HandoffFailure> {
    let dir = name_of(&checkout.path);
    let branch = &checkout.branch;
    match &checkout.state {
        WorktreeState::Current => Ok(None),
        WorktreeState::Dirty => Ok(Some(context::NOTE_DIRTY)),
        WorktreeState::Diverged => Ok(Some(context::NOTE_DIVERGED)),
        WorktreeState::OtherBranch(_) | WorktreeState::Detached if purpose == Purpose::Review => {
            Ok(Some(context::NOTE_OTHER_BRANCH_REVIEW))
        }
        WorktreeState::OtherBranch(other) => Err(HandoffFailure::plain(format!(
            "the worktree {dir} is on {other}, not on {branch}: switch it back first"
        ))),
        WorktreeState::Detached => Err(HandoffFailure::plain(format!(
            "the worktree {dir} is on a detached HEAD, not on {branch}: switch it back first"
        ))),
    }
}

/// The forge's reset for a rate limit, in Unix seconds, when it named one.
fn rate_limit_reset(error: &ForgeError) -> Option<i64> {
    match error {
        ForgeError::RateLimited { reset_at, .. } => *reset_at,
        _ => None,
    }
}

/// `<worktree>/.sirio/handoff`, created when missing. Each component must be
/// a real folder, not a symlink, so nothing written there lands outside the
/// worktree.
fn handoff_dir(worktree: &Path) -> io::Result<PathBuf> {
    let mut dir = worktree.to_path_buf();
    for part in HANDOFF_DIR.split('/') {
        dir.push(part);
        match std::fs::symlink_metadata(&dir) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => return Err(io::Error::other(format!("{} is not a folder", dir.display()))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => std::fs::create_dir(&dir)?,
            Err(error) => return Err(error),
        }
    }
    Ok(dir)
}

/// Writes one context file, refusing to write through a symlink planted at its name.
fn write_context(path: &Path, text: &str) -> io::Result<()> {
    if std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(io::Error::other(format!("{} is a symlink", path.display())));
    }
    std::fs::write(path, text)
}

/// Deletes all but the newest hand-off files in `dir`. Only regular `*.md`
/// files directly inside it are considered (a symlink is never followed), and
/// only the names `to_prune` returns are removed. Best effort: a file that
/// cannot be removed is kept, and the hand-off still succeeds.
fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let files: Vec<(String, SystemTime)> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.ends_with(".md") {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((name, modified))
        })
        .collect();
    for name in context::to_prune(files) {
        let _ = std::fs::remove_file(dir.join(name));
    }
}
