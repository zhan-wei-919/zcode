use super::message::AppMessage;
use crate::kernel::editor::ReloadRequest;
use crate::kernel::editor::TabId;
use crate::kernel::language::LanguageId;
use crate::kernel::services::ports::lsp::{line_len_chars, lsp_col_to_char_offset_in_line};
use crate::kernel::services::ports::DirEntryInfo;
use crate::kernel::services::ports::{
    LspPositionEncoding, LspResourceOp, LspTextEdit, LspWorkspaceFileEdit,
};
use crate::models::should_ignore;
use ropey::Rope;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use tree_sitter::Tree;

pub struct AsyncRuntime {
    runtime: tokio::runtime::Runtime,
    tx: Sender<AppMessage>,
}

impl AsyncRuntime {
    pub fn new(tx: Sender<AppMessage>) -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .or_else(|e| {
                tracing::error!(
                    error = %e,
                    "Failed to create multi-thread tokio runtime, falling back to current-thread"
                );
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
            })?;
        Ok(Self { runtime, tx })
    }

    pub fn tokio_handle(&self) -> tokio::runtime::Handle {
        self.runtime.handle().clone()
    }

    pub fn compute_syntax_highlights(
        &self,
        tab_id: TabId,
        version: u64,
        language: LanguageId,
        rope: Rope,
        tree: Tree,
        segments: Vec<(usize, usize)>,
    ) {
        let tx = self.tx.clone();
        self.runtime.spawn_blocking(move || {
            let patches =
                crate::kernel::editor::compute_highlight_patches(language, &tree, &rope, &segments);
            let _ = tx.send(AppMessage::SyntaxHighlightsComputed {
                tab_id,
                version,
                patches,
            });
        });
    }

    pub fn load_dir(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::read_dir(&path).await {
                Ok(mut entries) => {
                    let mut result = Vec::new();
                    loop {
                        let entry = match entries.next_entry().await {
                            Ok(Some(entry)) => entry,
                            Ok(None) => break,
                            Err(e) => {
                                let _ = tx.send(AppMessage::DirLoadError {
                                    path,
                                    error: e.to_string(),
                                });
                                return;
                            }
                        };

                        let name = entry.file_name().to_string_lossy().to_string();
                        if should_ignore(&name) {
                            continue;
                        }

                        if let Ok(file_type) = entry.file_type().await {
                            result.push(DirEntryInfo {
                                name,
                                is_dir: file_type.is_dir(),
                            });
                        }
                    }
                    let _ = tx.send(AppMessage::DirLoaded {
                        path,
                        entries: result,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::DirLoadError {
                        path,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn load_file(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => {
                    let _ = tx.send(AppMessage::FileLoaded { path, content });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FileError {
                        path,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn reload_file(&self, request: ReloadRequest) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            if let Ok(content) = tokio::fs::read_to_string(&request.path).await {
                let _ = tx.send(AppMessage::FileReloaded { request, content });
            }
        });
    }

    pub fn write_file(
        &self,
        pane: usize,
        path: PathBuf,
        version: u64,
        head: crate::models::OpId,
        rope: Rope,
    ) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let tx_for_error = tx.clone();
            let path_for_error = path.clone();
            let path_for_write = path.clone();
            let result =
                tokio::task::spawn_blocking(move || write_rope_to_path(&path_for_write, &rope))
                    .await;

            let success = match result {
                Ok(Ok(())) => true,
                Ok(Err(e)) => {
                    let _ = tx_for_error.send(AppMessage::FsOpError {
                        op: "write_file",
                        path: path_for_error,
                        to: None,
                        error: e.to_string(),
                    });
                    false
                }
                Err(e) => {
                    let _ = tx_for_error.send(AppMessage::FsOpError {
                        op: "write_file",
                        path: path_for_error,
                        to: None,
                        error: e.to_string(),
                    });
                    false
                }
            };

            let _ = tx.send(AppMessage::FileSaved {
                pane,
                path,
                success,
                version,
                head,
            });
        });
    }

    pub fn create_file(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .await
            {
                Ok(_) => {
                    let _ = tx.send(AppMessage::PathCreated {
                        path,
                        is_dir: false,
                    });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FsOpError {
                        op: "create_file",
                        path,
                        to: None,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn create_dir(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            match tokio::fs::create_dir(&path).await {
                Ok(_) => {
                    let _ = tx.send(AppMessage::PathCreated { path, is_dir: true });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FsOpError {
                        op: "create_dir",
                        path,
                        to: None,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn rename_path(&self, from: PathBuf, to: PathBuf, overwrite: bool) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let tx_for_error = tx.clone();
            let from_for_error = from.clone();
            let to_for_error = to.clone();
            let from_for_work = from.clone();
            let to_for_work = to.clone();

            let result = tokio::task::spawn_blocking(move || {
                move_path(from_for_work.as_path(), to_for_work.as_path(), overwrite)
            })
            .await;

            match result {
                Ok(Ok(())) => {
                    let _ = tx.send(AppMessage::PathRenamed { from, to });
                }
                Ok(Err(e)) => {
                    let _ = tx_for_error.send(AppMessage::FsOpError {
                        op: "rename_path",
                        path: from_for_error,
                        to: Some(to_for_error),
                        error: e.to_string(),
                    });
                }
                Err(e) => {
                    let _ = tx_for_error.send(AppMessage::FsOpError {
                        op: "rename_path",
                        path: from_for_error,
                        to: Some(to_for_error),
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn copy_path(&self, from: PathBuf, to: PathBuf, overwrite: bool) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let tx_for_error = tx.clone();
            let from_for_error = from.clone();
            let to_for_error = to.clone();
            let from_for_work = from.clone();
            let to_for_work = to.clone();

            let result = tokio::task::spawn_blocking(move || {
                let meta = std::fs::symlink_metadata(&from_for_work)?;
                copy_path(from_for_work.as_path(), to_for_work.as_path(), overwrite)?;
                Ok::<bool, io::Error>(meta.is_dir())
            })
            .await;

            match result {
                Ok(Ok(is_dir)) => {
                    let _ = tx.send(AppMessage::PathCreated { path: to, is_dir });
                }
                Ok(Err(e)) => {
                    let _ = tx_for_error.send(AppMessage::FsOpError {
                        op: "copy_path",
                        path: from_for_error,
                        to: Some(to_for_error),
                        error: e.to_string(),
                    });
                }
                Err(e) => {
                    let _ = tx_for_error.send(AppMessage::FsOpError {
                        op: "copy_path",
                        path: from_for_error,
                        to: Some(to_for_error),
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn delete_path(&self, path: PathBuf, is_dir: bool) {
        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            let result = if is_dir {
                tokio::fs::remove_dir_all(&path).await
            } else {
                tokio::fs::remove_file(&path).await
            };
            match result {
                Ok(_) => {
                    let _ = tx.send(AppMessage::PathDeleted { path });
                }
                Err(e) => {
                    let _ = tx.send(AppMessage::FsOpError {
                        op: "delete_path",
                        path,
                        to: None,
                        error: e.to_string(),
                    });
                }
            }
        });
    }

    pub fn apply_file_edits(
        &self,
        position_encoding: LspPositionEncoding,
        resource_ops: Vec<LspResourceOp>,
        edits: Vec<LspWorkspaceFileEdit>,
    ) {
        if resource_ops.is_empty() && edits.is_empty() {
            return;
        }

        let tx = self.tx.clone();
        self.runtime.spawn(async move {
            for op in resource_ops {
                let tx_for_error = tx.clone();
                match op {
                    LspResourceOp::CreateFile {
                        path,
                        overwrite,
                        ignore_if_exists,
                    } => {
                        let path_for_work = path.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            apply_create_file(path_for_work.as_path(), overwrite, ignore_if_exists)
                        })
                        .await;

                        match result {
                            Ok(Ok(())) => {
                                let _ = tx.send(AppMessage::PathCreated {
                                    path,
                                    is_dir: false,
                                });
                            }
                            Ok(Err(e)) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_create_file",
                                    path,
                                    to: None,
                                    error: e,
                                });
                            }
                            Err(e) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_create_file",
                                    path,
                                    to: None,
                                    error: e.to_string(),
                                });
                            }
                        }
                    }
                    LspResourceOp::RenameFile {
                        old_path,
                        new_path,
                        overwrite,
                        ignore_if_exists,
                    } => {
                        let from = old_path.clone();
                        let to = new_path.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            apply_rename_file(
                                from.as_path(),
                                to.as_path(),
                                overwrite,
                                ignore_if_exists,
                            )
                        })
                        .await;

                        match result {
                            Ok(Ok(())) => {
                                let _ = tx.send(AppMessage::PathRenamed {
                                    from: old_path,
                                    to: new_path,
                                });
                            }
                            Ok(Err(e)) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_rename_file",
                                    path: old_path,
                                    to: Some(new_path),
                                    error: e,
                                });
                            }
                            Err(e) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_rename_file",
                                    path: old_path,
                                    to: Some(new_path),
                                    error: e.to_string(),
                                });
                            }
                        }
                    }
                    LspResourceOp::DeleteFile {
                        path,
                        recursive,
                        ignore_if_not_exists,
                    } => {
                        let path_for_work = path.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            apply_delete_file(
                                path_for_work.as_path(),
                                recursive,
                                ignore_if_not_exists,
                            )
                        })
                        .await;

                        match result {
                            Ok(Ok(())) => {
                                let _ = tx.send(AppMessage::PathDeleted { path });
                            }
                            Ok(Err(e)) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_delete_file",
                                    path,
                                    to: None,
                                    error: e,
                                });
                            }
                            Err(e) => {
                                let _ = tx_for_error.send(AppMessage::FsOpError {
                                    op: "apply_delete_file",
                                    path,
                                    to: None,
                                    error: e.to_string(),
                                });
                            }
                        }
                    }
                }
            }

            for file_edit in edits {
                if file_edit.edits.is_empty() {
                    continue;
                }

                let tx_for_error = tx.clone();
                let path = file_edit.path;
                let edits = file_edit.edits;
                let path_for_work = path.clone();

                let result = tokio::task::spawn_blocking(move || {
                    apply_text_edits_to_path(path_for_work.as_path(), &edits, position_encoding)
                })
                .await;

                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        let _ = tx_for_error.send(AppMessage::FsOpError {
                            op: "apply_file_edits",
                            path,
                            to: None,
                            error: e,
                        });
                    }
                    Err(e) => {
                        let _ = tx_for_error.send(AppMessage::FsOpError {
                            op: "apply_file_edits",
                            path,
                            to: None,
                            error: e.to_string(),
                        });
                    }
                }
            }
        });
    }
}

fn copy_path(from: &std::path::Path, to: &std::path::Path, overwrite: bool) -> io::Result<()> {
    if from == to {
        return Ok(());
    }

    let from_meta = std::fs::symlink_metadata(from)?;
    if from_meta.is_dir() && to.starts_with(from) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot copy a directory into itself",
        ));
    }

    let Some(parent) = to.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination has no parent directory",
        ));
    };
    if !std::fs::metadata(parent).is_ok_and(|m| m.is_dir()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination parent is not a directory",
        ));
    }

    if !overwrite && std::fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination exists",
        ));
    }

    if overwrite {
        remove_existing_path(to)?;
    }

    if from_meta.file_type().is_symlink() {
        return copy_symlink(from, to);
    }

    if from_meta.is_dir() {
        return copy_dir_recursive(from, to);
    }

    std::fs::copy(from, to)?;
    Ok(())
}

fn move_path(from: &std::path::Path, to: &std::path::Path, overwrite: bool) -> io::Result<()> {
    move_path_impl(from, to, overwrite, |from, to| std::fs::rename(from, to))
}

fn move_path_impl(
    from: &std::path::Path,
    to: &std::path::Path,
    overwrite: bool,
    rename_fn: impl FnOnce(&std::path::Path, &std::path::Path) -> io::Result<()>,
) -> io::Result<()> {
    if from == to {
        return Ok(());
    }

    let from_meta = std::fs::symlink_metadata(from)?;
    if from_meta.is_dir() && to.starts_with(from) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot move a directory into itself",
        ));
    }

    let Some(parent) = to.parent() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination has no parent directory",
        ));
    };
    if !std::fs::metadata(parent).is_ok_and(|m| m.is_dir()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "destination parent is not a directory",
        ));
    }

    if !overwrite && std::fs::symlink_metadata(to).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "destination exists",
        ));
    }

    if overwrite {
        remove_existing_path(to)?;
    }

    match rename_fn(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => move_across_devices(from, to),
        Err(e) => Err(e),
    }
}

fn remove_existing_path(path: &std::path::Path) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.is_dir() {
                std::fs::remove_dir_all(path)
            } else {
                std::fs::remove_file(path)
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn move_across_devices(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    let from_meta = std::fs::symlink_metadata(from)?;
    if from_meta.file_type().is_symlink() {
        copy_symlink(from, to)?;
        std::fs::remove_file(from)?;
        return Ok(());
    }

    if from_meta.is_dir() {
        copy_dir_recursive(from, to)?;
        std::fs::remove_dir_all(from)?;
        return Ok(());
    }

    std::fs::copy(from, to)?;
    std::fs::remove_file(from)?;
    Ok(())
}

fn copy_dir_recursive(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    std::fs::create_dir(to)?;

    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let from_child = entry.path();
        let to_child = to.join(entry.file_name());
        let meta = std::fs::symlink_metadata(&from_child)?;

        if meta.file_type().is_symlink() {
            copy_symlink(&from_child, &to_child)?;
        } else if meta.is_dir() {
            copy_dir_recursive(&from_child, &to_child)?;
        } else {
            std::fs::copy(&from_child, &to_child)?;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn copy_symlink(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    let target = std::fs::read_link(from)?;
    std::os::unix::fs::symlink(target, to)
}

#[cfg(windows)]
fn copy_symlink(from: &std::path::Path, to: &std::path::Path) -> io::Result<()> {
    let target = std::fs::read_link(from)?;
    match std::fs::metadata(from) {
        Ok(meta) if meta.is_dir() => std::os::windows::fs::symlink_dir(target, to),
        _ => std::os::windows::fs::symlink_file(target, to),
    }
}

#[cfg(not(any(unix, windows)))]
fn copy_symlink(_from: &std::path::Path, _to: &std::path::Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "symlinks are not supported on this platform",
    ))
}

fn apply_create_file(
    path: &std::path::Path,
    overwrite: bool,
    ignore_if_exists: bool,
) -> Result<(), String> {
    match std::fs::metadata(path) {
        Ok(meta) => {
            if ignore_if_exists {
                return Ok(());
            }
            if meta.is_dir() {
                return Err("path exists and is a directory".to_string());
            }
            if overwrite {
                std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(path)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            } else {
                Err("path exists".to_string())
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map(|_| ())
            .map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

fn apply_rename_file(
    old_path: &std::path::Path,
    new_path: &std::path::Path,
    overwrite: bool,
    ignore_if_exists: bool,
) -> Result<(), String> {
    if ignore_if_exists && std::fs::metadata(new_path).is_ok() {
        return Ok(());
    }

    if overwrite {
        if let Ok(meta) = std::fs::metadata(new_path) {
            if meta.is_dir() {
                std::fs::remove_dir_all(new_path).map_err(|e| e.to_string())?;
            } else {
                std::fs::remove_file(new_path).map_err(|e| e.to_string())?;
            }
        }
    }

    std::fs::rename(old_path, new_path).map_err(|e| e.to_string())
}

fn apply_delete_file(
    path: &std::path::Path,
    recursive: bool,
    ignore_if_not_exists: bool,
) -> Result<(), String> {
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if ignore_if_not_exists {
                return Ok(());
            }
            return Err(e.to_string());
        }
        Err(e) => return Err(e.to_string()),
    };

    if meta.is_dir() {
        if recursive {
            std::fs::remove_dir_all(path).map_err(|e| e.to_string())
        } else {
            std::fs::remove_dir(path).map_err(|e| e.to_string())
        }
    } else {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

fn write_rope_to_path(path: &std::path::Path, rope: &Rope) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    for chunk in rope.chunks() {
        writer.write_all(chunk.as_bytes())?;
    }
    writer.flush()
}

fn apply_text_edits_to_path(
    path: &std::path::Path,
    edits: &[LspTextEdit],
    encoding: LspPositionEncoding,
) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut rope = Rope::from_str(&content);
    apply_text_edits_to_rope(&mut rope, edits, encoding);
    write_rope_to_path(path, &rope).map_err(|e| e.to_string())
}

fn apply_text_edits_to_rope(rope: &mut Rope, edits: &[LspTextEdit], encoding: LspPositionEncoding) {
    let mut ordered: Vec<&LspTextEdit> = edits.iter().collect();
    ordered.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then_with(|| b.range.start.character.cmp(&a.range.start.character))
            .then_with(|| b.range.end.line.cmp(&a.range.end.line))
            .then_with(|| b.range.end.character.cmp(&a.range.end.character))
    });

    for edit in ordered {
        let start_byte = lsp_position_to_byte_offset(
            rope,
            edit.range.start.line,
            edit.range.start.character,
            encoding,
        );
        let end_byte = lsp_position_to_byte_offset(
            rope,
            edit.range.end.line,
            edit.range.end.character,
            encoding,
        );

        if start_byte == end_byte && edit.new_text.is_empty() {
            continue;
        }

        let len_bytes = rope.len_bytes();
        let start_byte = start_byte.min(len_bytes);
        let end_byte = end_byte.min(len_bytes);

        let mut start_char = rope.byte_to_char(start_byte);
        let mut end_char = rope.byte_to_char(end_byte);
        if start_char > end_char {
            std::mem::swap(&mut start_char, &mut end_char);
        }

        rope.remove(start_char..end_char);
        if !edit.new_text.is_empty() {
            rope.insert(start_char, &edit.new_text);
        }
    }
}

fn lsp_position_to_byte_offset(
    rope: &Rope,
    line: u32,
    column: u32,
    encoding: LspPositionEncoding,
) -> usize {
    if rope.len_chars() == 0 {
        return 0;
    }

    let line_index = (line as usize).min(rope.len_lines().saturating_sub(1));
    let line_slice = rope.line(line_index);
    let col_chars = lsp_col_to_char_offset_in_line(line_slice, column, encoding);
    let line_start = rope.line_to_char(line_index);
    let line_len = line_len_chars(line_slice);
    let char_offset = (line_start + col_chars.min(line_len)).min(rope.len_chars());
    rope.char_to_byte(char_offset)
}

#[cfg(test)]
#[path = "../../../../../tests/unit/kernel/services/adapters/runtime/async_runtime.rs"]
mod tests;
