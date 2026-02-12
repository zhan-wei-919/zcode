use std::path::{Path, PathBuf};

use crate::kernel::language::LanguageId;
use crate::kernel::services::ports::LspClientKey;

pub fn is_lsp_source_path(path: &Path) -> bool {
    LanguageId::from_path(path)
        .and_then(|l| l.server_kind())
        .is_some()
}

pub fn client_key_for_path(
    workspace_root: &Path,
    path: &Path,
) -> Option<(LanguageId, LspClientKey)> {
    let language = LanguageId::from_path(path)?;
    let server = language.server_kind()?;
    let root = language_root_for_file(workspace_root, language, path);
    Some((language, LspClientKey { server, root }))
}

pub fn language_root_for_file(
    workspace_root: &Path,
    language: LanguageId,
    file_path: &Path,
) -> PathBuf {
    let Some(start_dir) = file_path.parent() else {
        return workspace_root.to_path_buf();
    };

    if !file_path.starts_with(workspace_root) {
        return workspace_root.to_path_buf();
    }

    find_nearest_marker_root(workspace_root, start_dir, language.markers())
}

fn find_nearest_marker_root(workspace_root: &Path, start_dir: &Path, markers: &[&str]) -> PathBuf {
    let mut cur = start_dir;
    loop {
        if cur.starts_with(workspace_root)
            && markers
                .iter()
                .any(|name| cur.join(name).try_exists().unwrap_or(false))
        {
            return cur.to_path_buf();
        }

        if cur == workspace_root {
            break;
        }
        match cur.parent() {
            Some(parent) => cur = parent,
            None => break,
        }
    }

    workspace_root.to_path_buf()
}
