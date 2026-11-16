//! `getDiagnostics`: Claude's request answered from what the language servers already told the
//! drawers (spec 2026-11-15, 4.6 and 6).
//!
//! The read happens on the UI thread and only from memory: the drawers' problem stores are
//! looked up (never created: a worktree with no drawer has no findings), and nothing waits for a
//! language server. Claude asks per file before an edit (its baseline) and for the whole
//! workspace after it, so an empty answer is correct only when there truly are no findings. The
//! bridge's own 400 ms deadline answers with an error, never `[]`, when this does not run in time.
//!
//! Nothing here logs a path, a message or a finding.

use std::path::{Path, PathBuf};

use gpui::{App, Context};
use sirio_ide::{ConnId, DiagId, Diagnostic, FileDiagnostics, Pos16, Severity};
use sirio_ui::drawer::{StoredProblem, WorktreeDrawer};

use super::IdeBridge;
use crate::{SirioWorkspace, paths_name_the_same_document};

/// Sirio's severity in the protocol's words (spec 1.4).
fn severity(severity: sirio_lsp::Severity) -> Severity {
    match severity {
        sirio_lsp::Severity::Error => Severity::Error,
        sirio_lsp::Severity::Warning => Severity::Warning,
        sirio_lsp::Severity::Information => Severity::Info,
        sirio_lsp::Severity::Hint => Severity::Hint,
    }
}

/// The store's positions are already 0-based lines and UTF-16 columns, as the protocol counts.
fn diagnostic(problem: &StoredProblem) -> Diagnostic {
    Diagnostic {
        message: problem.message.clone(),
        severity: severity(problem.severity),
        start: Pos16 {
            line: problem.line,
            character: problem.column,
        },
        end: Pos16 {
            line: problem.end_line,
            character: problem.end_column,
        },
        source: problem.source.clone(),
        code: None,
    }
}

impl SirioWorkspace {
    /// The drawer made earlier for `worktree`, looked up and never created.
    fn drawer_if_any(&self, worktree: &Path) -> Option<&gpui::Entity<WorktreeDrawer>> {
        self.drawers.get(worktree).or_else(|| {
            self.drawers
                .iter()
                .find(|(path, _)| paths_name_the_same_document(path, worktree))
                .map(|(_, drawer)| drawer)
        })
    }

    /// Every language server's findings for the file at the absolute path `path`, already mapped
    /// to the protocol's severities (`Error`, `Warning`, `Info`, `Hint`) and to 0-based lines and
    /// UTF-16 columns, read from memory (the drawer's problem store) without waiting for a
    /// server. Empty when the file lies in no worktree, its worktree has no drawer, or no server
    /// reported it. This is the read the native chat shares (spec 6): it takes an absolute path
    /// and never a URI.
    pub(crate) fn ide_diagnostics_for(&self, path: &Path, cx: &App) -> Vec<Diagnostic> {
        let Some((root, relative)) = self.worktree_and_relative(path) else {
            return Vec::new();
        };
        let Some(drawer) = self.drawer_if_any(&root) else {
            return Vec::new();
        };
        drawer
            .read(cx)
            .problems_for(&relative)
            .iter()
            .map(diagnostic)
            .collect()
    }

    /// Every file with findings in `worktree`, or in every worktree that has a drawer when there
    /// is none, each with its absolute path, in path order.
    pub(crate) fn ide_diagnostics_in(&self, worktree: Option<&Path>, cx: &App) -> Vec<FileDiagnostics> {
        let mut files = Vec::new();
        let mut collect = |root: &Path, drawer: &gpui::Entity<WorktreeDrawer>| {
            for (relative, problems) in drawer.read(cx).all_problems() {
                if problems.is_empty() {
                    continue;
                }
                files.push(FileDiagnostics {
                    path: root.join(relative),
                    diagnostics: problems.iter().map(diagnostic).collect(),
                });
            }
        };
        match worktree {
            Some(root) => {
                if let Some(drawer) = self.drawer_if_any(root) {
                    collect(root, drawer);
                }
            }
            None => {
                for (root, drawer) in &self.drawers {
                    collect(root, drawer);
                }
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files
    }

    /// Answers one `getDiagnostics`. With a file it does not depend on who owns the connection
    /// (an ownerless one gets diagnostics too), so it answers at once. Without one it is the
    /// owner's worktree, else the worktree the connection's cwd resolved to, else every
    /// worktree: it goes through `ide_when_resolved` so that a connection still ownerless is
    /// resolved first (spec 10.3). If the bridge's deadline passes before that, the error it
    /// sent stands and this answer finds nothing waiting.
    pub(crate) fn ide_answer_diagnostics(
        &mut self,
        id: DiagId,
        conn: ConnId,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        match path {
            Some(path) => {
                let diagnostics = self.ide_diagnostics_for(&path, cx);
                let files = if diagnostics.is_empty() {
                    Vec::new()
                } else {
                    vec![FileDiagnostics { path, diagnostics }]
                };
                answer(cx, &id, files);
            }
            None => self.ide_when_resolved(cx, move |workspace, cx| {
                let worktree = cx
                    .try_global::<IdeBridge>()
                    .and_then(|bridge| bridge.connections.get(&conn))
                    .and_then(|connection| connection.worktree.clone());
                let files = workspace.ide_diagnostics_in(worktree.as_deref(), cx);
                answer(cx, &id, files);
            }),
        }
    }
}

fn answer(cx: &App, id: &DiagId, files: Vec<FileDiagnostics>) {
    if let Some(bridge) = cx.try_global::<IdeBridge>().and_then(IdeBridge::bridge) {
        bridge.answer_diagnostics(id, files);
    }
}
