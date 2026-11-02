//! Saved prompts (spec `2026-11-01-prompt-list-design.md`): the gpui layer
//! over [`store`], which owns the folder.
//!
//! The host sets [`PromptsDir`] once at startup; without it there is no
//! store, every write answers `NotFound`, and no path is a prompt. Every
//! write Sirio makes bumps [`PromptsRevision`], which the Prompts view
//! observes to read the folder again.
//!
//! An open prompt editor registers itself as a [`LivePrompt`], so Copy can
//! flush a pending autosave and copy what the editor shows, and Delete can
//! stop a pending save from writing the file back. Those calls update the
//! editors, so [`copy_text`], [`delete`] and [`flush_all`] must never run
//! inside an editor's own update; [`remove_if_blank`] touches no editor and
//! may.

pub mod store;

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{App, Global};

pub use store::{PromptEntry, PromptStore};

/// The prompts folder, set by the host before any view is built.
pub struct PromptsDir(pub PathBuf);

impl Global for PromptsDir {}

/// Bumped on every write Sirio makes to the folder.
pub struct PromptsRevision(pub u64);

impl Global for PromptsRevision {}

/// An open prompt editor, as the list sees it.
pub trait LivePrompt {
    /// The editor's current text, or `None` once the editor is gone.
    fn text(&self, cx: &App) -> Option<String>;
    /// Writes any pending text and title now.
    fn flush(&self, cx: &mut App);
    /// Drops any pending save; every later save is a no-op.
    fn forget(&self, cx: &mut App);
}

/// Open prompt editors by file. `Rc` so a caller can take its own handles
/// out of the global before calling back into an editor, which may itself
/// touch the globals.
#[derive(Default)]
struct LivePrompts(HashMap<PathBuf, Vec<Rc<dyn LivePrompt>>>);

impl Global for LivePrompts {}

fn not_found() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "no prompts folder")
}

/// The registry's key for a path: the file as the disk names it, so the
/// list's path and an editor's path meet even when they were spelled
/// differently. A path that no longer resolves keys as given.
fn key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn store(cx: &App) -> Option<PromptStore> {
    cx.try_global::<PromptsDir>().map(|dir| PromptStore::new(dir.0.clone()))
}

pub fn is_prompt_path(path: &Path, cx: &App) -> bool {
    store(cx).is_some_and(|store| store.owns(path))
}

pub fn changed(cx: &mut App) {
    let next = cx.try_global::<PromptsRevision>().map_or(1, |revision| revision.0 + 1);
    cx.set_global(PromptsRevision(next));
}

pub fn create(cx: &mut App) -> io::Result<PathBuf> {
    let entry = store(cx).ok_or_else(not_found)?.create()?;
    changed(cx);
    Ok(entry.path)
}

pub fn retitle(path: &Path, title: &str, cx: &mut App) -> io::Result<()> {
    store(cx).ok_or_else(not_found)?.retitle(path, title)?;
    changed(cx);
    Ok(())
}

pub fn register_live(path: PathBuf, live: Box<dyn LivePrompt>, cx: &mut App) {
    let mut registry = cx.try_global::<LivePrompts>().map(|l| l.0.clone()).unwrap_or_default();
    registry.entry(key(&path)).or_default().push(Rc::from(live));
    cx.set_global(LivePrompts(registry));
}

/// The prompt's full text. An open editor is flushed first and its text is
/// returned, so a keystroke still waiting for its autosave is copied too;
/// otherwise the file's text, unchanged.
pub fn copy_text(path: &Path, cx: &mut App) -> io::Result<String> {
    for live in live_for(path, cx) {
        live.flush(cx);
        if let Some(text) = live.text(cx) {
            return Ok(text);
        }
    }
    std::fs::read_to_string(path)
}

/// Deletes the prompt, then forgets every open editor of it, so a pending
/// autosave cannot write the file back. Both happen in this one call, so no
/// autosave runs between them. A delete that fails forgets nothing: its
/// editors keep saving.
pub fn delete(path: &Path, cx: &mut App) -> io::Result<()> {
    let store = store(cx).ok_or_else(not_found)?;
    // Both read while the file is there: the registry keys by its
    // canonical path, which a missing file no longer has.
    let key = key(path);
    let editors = live_for(path, cx);
    store.delete(path)?;
    for live in editors {
        live.forget(cx);
    }
    if let Some(mut registry) = cx.try_global::<LivePrompts>().map(|l| l.0.clone()) {
        registry.remove(&key);
        cx.set_global(LivePrompts(registry));
    }
    changed(cx);
    Ok(())
}

/// Deletes an untitled prompt whose text is empty or only whitespace, so
/// pressing New and backing out leaves nothing behind.
pub fn remove_if_blank(path: &Path, cx: &mut App) {
    let Some(store) = store(cx) else {
        return;
    };
    if !store.title(path).is_empty() {
        return;
    }
    let blank = std::fs::read_to_string(path).is_ok_and(|text| text.trim().is_empty());
    if blank && store.delete(path).is_ok() {
        changed(cx);
    }
}

pub fn flush_all(cx: &mut App) {
    prune(cx);
    let every: Vec<Rc<dyn LivePrompt>> = cx
        .try_global::<LivePrompts>()
        .map(|l| l.0.values().flatten().cloned().collect())
        .unwrap_or_default();
    for live in every {
        live.flush(cx);
    }
}

/// The live editors of `path`, taken out of the global so each can be
/// called with the whole `App`.
fn live_for(path: &Path, cx: &mut App) -> Vec<Rc<dyn LivePrompt>> {
    prune(cx);
    cx.try_global::<LivePrompts>()
        .and_then(|l| l.0.get(&key(path)).cloned())
        .unwrap_or_default()
}

/// Drops the editors that are gone.
fn prune(cx: &mut App) {
    let Some(live) = cx.try_global::<LivePrompts>() else {
        return;
    };
    let kept: HashMap<PathBuf, Vec<Rc<dyn LivePrompt>>> = live
        .0
        .iter()
        .map(|(path, editors)| {
            let alive = editors.iter().filter(|e| e.text(cx).is_some()).cloned().collect::<Vec<_>>();
            (path.clone(), alive)
        })
        .filter(|(_, editors)| !editors.is_empty())
        .collect();
    cx.set_global(LivePrompts(kept));
}
