use std::path::{Path, PathBuf};

use agent_core::{AbortSignal, CoreError};
use ignore::{
    Match,
    gitignore::{Gitignore, GitignoreBuilder},
};

pub(crate) enum CandidateCollection {
    Completed(Vec<PathBuf>),
    Cancelled,
}

struct PendingDirectory {
    path: PathBuf,
    matchers: Vec<Gitignore>,
}

pub(crate) async fn collect_candidate_files(
    base: &Path,
    abort: &AbortSignal,
    mut predicate: impl FnMut(&Path, &Path) -> bool,
) -> Result<CandidateCollection, CoreError> {
    let mut pending = vec![PendingDirectory {
        path: base.to_path_buf(),
        matchers: load_matchers_for_directory(base).await,
    }];
    let mut candidates = Vec::new();

    while let Some(directory) = pending.pop() {
        if abort.is_aborted() {
            return Ok(CandidateCollection::Cancelled);
        }

        let mut entries = match tokio::fs::read_dir(&directory.path).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            if abort.is_aborted() {
                return Ok(CandidateCollection::Cancelled);
            }

            let path = entry.path();
            let file_type = match entry.file_type().await {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            let is_dir = file_type.is_dir();
            let name = entry.file_name();
            let name = name.to_string_lossy();

            if is_dir && super::should_skip_directory_name(name.as_ref()) {
                continue;
            }

            if is_ignored(&directory.matchers, &path, is_dir) {
                continue;
            }

            if is_dir {
                let mut matchers = directory.matchers.clone();
                matchers.extend(load_matchers_for_directory(&path).await);
                pending.push(PendingDirectory { path, matchers });
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let relative = path.strip_prefix(base).unwrap_or(&path);
            if predicate(relative, &path) {
                candidates.push(path);
            }
        }
    }

    Ok(CandidateCollection::Completed(candidates))
}

async fn load_matchers_for_directory(path: &Path) -> Vec<Gitignore> {
    let gitignore_path = path.join(".gitignore");
    let contents = match tokio::fs::read_to_string(&gitignore_path).await {
        Ok(contents) => contents,
        Err(_) => return Vec::new(),
    };

    let mut builder = GitignoreBuilder::new(path);
    for line in contents.lines() {
        let _ = builder.add_line(Some(gitignore_path.clone()), line);
    }

    match builder.build() {
        Ok(gitignore) if !gitignore.is_empty() => vec![gitignore],
        _ => Vec::new(),
    }
}

fn is_ignored(matchers: &[Gitignore], path: &Path, is_dir: bool) -> bool {
    let mut matched = Match::None;
    for matcher in matchers {
        match matcher.matched(path, is_dir) {
            Match::None => {}
            Match::Ignore(_) => matched = Match::Ignore(()),
            Match::Whitelist(_) => matched = Match::Whitelist(()),
        }
    }
    matched.is_ignore()
}
