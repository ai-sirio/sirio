//! What Sirio would do with one file, run against the real `PATH`.
//!
//! `sirio_apply`'s `update_probe` exists for the same reason: the interesting
//! part of this chain is a real process, and no test that stubs the process
//! can tell "there is no server" from "the server said no". Those two produce
//! the same empty context menu, and telling them apart by using the app is
//! exactly what nobody can do — which is how 0.18.0 shipped a Java file that
//! read `No language server offers definitions here` on a machine whose only
//! problem was a missing `jdtls`.
//!
//! Reads the same `languages.toml` the app reads, falling back to the same
//! compiled-in defaults, and resolves the project root the same way.
//!
//! ```text
//! cargo run -p sirio_lsp --example lsp_probe -- <worktree-root> <file> <line> <character>
//! ```
//! `line` and `character` are the protocol's: zero-based, and `character`
//! counts UTF-16 units.

use std::path::PathBuf;

use futures::executor::block_on;
use sirio_lsp::{lsp_types::Position, ConfigLoader, LspError, Server};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut next = |what: &str| {
        args.next()
            .unwrap_or_else(|| panic!("usage: lsp_probe <worktree-root> <file> <line> <character>; missing {what}"))
    };
    let worktree_root = PathBuf::from(next("worktree-root"));
    let file = PathBuf::from(next("file"));
    let line: u32 = next("line").parse().expect("line is a number");
    let character: u32 = next("character").parse().expect("character is a number");

    // The same resolution the app performs: `$SIRIO_CONFIG_DIR`, else
    // `$XDG_CONFIG_HOME/sirio`, else `~/.config/sirio`.
    let environment: std::collections::BTreeMap<String, String> = std::env::vars().collect();
    let home = PathBuf::from(environment.get("HOME").map_or("/", String::as_str));
    let config = sirio_lsp::config_path(&environment, &home);
    println!("table                 {}", config.display());
    let mut loader = ConfigLoader::new(config);
    loader.refresh();
    let Some(entry) = loader.table().for_path(&file).cloned() else {
        println!("entry                 NONE — no language claims this extension");
        return;
    };
    let root = sirio_lsp::project_root(&file, &worktree_root, &entry.roots);
    println!("language              {}", entry.name);
    println!("command               {} {:?}", entry.command, entry.args);
    println!("project root          {}", root.display());

    let (server, read_loop) = match block_on(Server::launch(&entry.command, &entry.args, &root)) {
        Ok(pair) => pair,
        Err(error) => {
            // Printed rather than returned, because which of these it is
            // decides what the reader has to do next: install something,
            // fix a `languages.toml` line, or file a bug.
            println!("launch                {error}");
            if matches!(error, LspError::NotInstalled { .. }) {
                println!("definition_available  false (nothing to ask)");
                println!("references_available  false (nothing to ask)");
            }
            return;
        }
    };
    let pump = std::thread::spawn(move || block_on(read_loop));

    let offered = server.capabilities();
    println!("launch                OK");
    println!("definition_available  {}", offered.definition);
    println!("references_available  {}", offered.references);

    let text = std::fs::read_to_string(&file).expect("the file is readable");
    block_on(sirio_lsp::did_open(
        server.client(),
        &file,
        &entry.name,
        1,
        &text,
    ))
    .expect("didOpen is a notification; it cannot be refused");

    let position = Position { line, character };
    match block_on(sirio_lsp::definition(server.client(), &file, position)) {
        Ok(found) => println!("definition            {} target(s) {found:?}", found.len()),
        Err(error) => println!("definition            {error}"),
    }
    match block_on(sirio_lsp::references(server.client(), &file, position)) {
        Ok(found) => println!("references            {} target(s) {found:?}", found.len()),
        Err(error) => println!("references            {error}"),
    }

    drop(server);
    let _ = pump.join();
}
