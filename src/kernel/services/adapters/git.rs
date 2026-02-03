use crate::kernel::git::{
    GitFileStatus, GitFileStatusKind, GitGutterMarkKind, GitGutterMarkRange, GitGutterMarks,
};
use crate::kernel::{GitHead, GitWorktreeItem};
use std::path::{Path, PathBuf};

pub fn parse_worktree_list(text: &str) -> Vec<GitWorktreeItem> {
    let mut items = Vec::new();

    let mut path: Option<PathBuf> = None;
    let mut head_sha: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut detached = false;

    let flush = |items: &mut Vec<GitWorktreeItem>,
                 path: &mut Option<PathBuf>,
                 head_sha: &mut Option<String>,
                 branch: &mut Option<String>,
                 detached: &mut bool| {
        let Some(path_val) = path.take() else {
            return;
        };
        let short_commit = head_sha.take().unwrap_or_default();
        let head = GitHead {
            branch: branch.take(),
            short_commit,
            detached: *detached,
        };
        *detached = false;
        items.push(GitWorktreeItem {
            path: path_val,
            head,
        });
    };

    for raw in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(rest) = raw.strip_prefix("worktree ") {
            flush(
                &mut items,
                &mut path,
                &mut head_sha,
                &mut branch,
                &mut detached,
            );
            path = Some(PathBuf::from(rest.trim()));
            continue;
        }
        if let Some(rest) = raw.strip_prefix("HEAD ") {
            head_sha = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = raw.strip_prefix("branch ") {
            let b = rest.trim();
            let b = b.strip_prefix("refs/heads/").unwrap_or(b);
            branch = Some(b.to_string());
            detached = false;
            continue;
        }
        if raw == "detached" {
            branch = None;
            detached = true;
            continue;
        }
    }

    flush(
        &mut items,
        &mut path,
        &mut head_sha,
        &mut branch,
        &mut detached,
    );
    items
}

pub fn parse_branch_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

pub fn parse_status_porcelain_z(data: &[u8], repo_root: &Path) -> Vec<(PathBuf, GitFileStatus)> {
    let mut out = Vec::new();
    let mut tokens = data.split(|b| *b == 0).filter(|t| !t.is_empty()).peekable();
    while let Some(token) = tokens.next() {
        if token.len() < 4 {
            continue;
        }

        let x = token[0] as char;
        let y = token[1] as char;
        if x == '!' && y == '!' {
            continue;
        }
        if x == ' ' && y == ' ' {
            continue;
        }
        if token[2] != b' ' {
            continue;
        }

        let path1_raw = &token[3..];
        let path1 = PathBuf::from(String::from_utf8_lossy(path1_raw).to_string());

        let mut path = path1;

        if x == 'R' || x == 'C' {
            if let Some(next) = tokens.next() {
                let path2 = PathBuf::from(String::from_utf8_lossy(next).to_string());
                path = path2;
            }
        }

        out.push((repo_root.join(path), status_from_xy(x, y)));
    }
    out
}

fn status_from_xy(x: char, y: char) -> GitFileStatus {
    if x == 'U' || y == 'U' || (x == 'A' && y == 'A') || (x == 'D' && y == 'D') {
        return GitFileStatus {
            index: Some(GitFileStatusKind::Conflict),
            worktree: Some(GitFileStatusKind::Conflict),
        };
    }

    if x == '?' && y == '?' {
        return GitFileStatus {
            index: Some(GitFileStatusKind::Untracked),
            worktree: Some(GitFileStatusKind::Untracked),
        };
    }

    GitFileStatus {
        index: kind_from_status_char(x),
        worktree: kind_from_status_char(y),
    }
}

fn kind_from_status_char(ch: char) -> Option<GitFileStatusKind> {
    match ch {
        ' ' => None,
        '?' => Some(GitFileStatusKind::Untracked),
        'A' => Some(GitFileStatusKind::Added),
        'U' => Some(GitFileStatusKind::Conflict),
        'M' | 'R' | 'C' | 'D' => Some(GitFileStatusKind::Modified),
        _ => Some(GitFileStatusKind::Modified),
    }
}

pub fn parse_diff_hunks_to_gutter_marks(text: &str) -> GitGutterMarks {
    let mut marks = GitGutterMarks::default();

    for line in text.lines() {
        let Some((_old_start, old_len, new_start, new_len)) = parse_unified_hunk_header(line)
        else {
            continue;
        };

        let new_line = new_start.saturating_sub(1);
        if old_len == 0 && new_len > 0 {
            marks.ranges.push(GitGutterMarkRange {
                start_line: new_line,
                end_line_exclusive: new_line.saturating_add(new_len),
                kind: GitGutterMarkKind::Added,
            });
        } else if new_len == 0 && old_len > 0 {
            marks.deletions.push(new_line);
        } else if new_len > 0 {
            marks.ranges.push(GitGutterMarkRange {
                start_line: new_line,
                end_line_exclusive: new_line.saturating_add(new_len),
                kind: GitGutterMarkKind::Modified,
            });
        }
    }

    normalize_gutter_marks(&mut marks);
    marks
}

fn parse_unified_hunk_header(line: &str) -> Option<(usize, usize, usize, usize)> {
    let line = line.trim_start();
    if !line.starts_with("@@") {
        return None;
    }

    let inner = line.split("@@").nth(1)?.trim();
    let mut parts = inner.split_whitespace();
    let old = parts.next()?;
    let new = parts.next()?;
    if !old.starts_with('-') || !new.starts_with('+') {
        return None;
    }

    let (old_start, old_len) = parse_range_token(old, '-')?;
    let (new_start, new_len) = parse_range_token(new, '+')?;
    Some((old_start, old_len, new_start, new_len))
}

fn parse_range_token(token: &str, prefix: char) -> Option<(usize, usize)> {
    let rest = token.strip_prefix(prefix)?;
    let (start_str, len_str) = rest.split_once(',').unwrap_or((rest, "1"));
    let start = start_str.parse::<usize>().ok()?;
    let len = len_str.parse::<usize>().ok().unwrap_or(1);
    Some((start, len))
}

fn normalize_gutter_marks(marks: &mut GitGutterMarks) {
    marks
        .ranges
        .sort_by_key(|r| (r.start_line, r.end_line_exclusive));
    let mut merged: Vec<GitGutterMarkRange> = Vec::with_capacity(marks.ranges.len());
    for r in marks.ranges.drain(..) {
        if let Some(prev) = merged.last_mut() {
            if prev.kind == r.kind && r.start_line <= prev.end_line_exclusive {
                prev.end_line_exclusive = prev.end_line_exclusive.max(r.end_line_exclusive);
                continue;
            }
        }
        merged.push(r);
    }
    marks.ranges = merged;

    marks.deletions.sort_unstable();
    marks.deletions.dedup();
}
