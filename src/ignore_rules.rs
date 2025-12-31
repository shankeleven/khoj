//! Loads .khojignore patterns and provides a matcher for skipping ignored paths.

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::path::Path;
use std::sync::OnceLock;

/// Global ignore matcher (built once per run).
static IGNORER: OnceLock<Gitignore> = OnceLock::new();

/// Initializes the ignorer from `.khojignore` at `root`.
/// Call this once at startup. Safe to call multiple times; only the first call builds.
pub fn init(root: &Path) {
    IGNORER.get_or_init(|| build_ignorer(root));
}

fn build_ignorer(root: &Path) -> Gitignore {
    let khojignore = root.join(".khojignore");
    let mut builder = GitignoreBuilder::new(root);
    if khojignore.is_file() {
        if let Some(err) = builder.add(&khojignore) {
            eprintln!("WARN: could not parse .khojignore: {err}");
        }
    }
    builder.build().unwrap_or_else(|e| {
        eprintln!("WARN: failed to build ignore rules: {e}");
        Gitignore::empty()
    })
}

/// Returns `true` if `path` should be ignored according to `.khojignore`.
/// `is_dir` should indicate whether the path is a directory.
pub fn is_ignored(path: &Path, is_dir: bool) -> bool {
    IGNORER
        .get()
        .map(|ig| ig.matched(path, is_dir).is_ignore())
        .unwrap_or(false)
}
