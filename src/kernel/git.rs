use rustc_hash::FxHashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHead {
    pub branch: Option<String>,
    pub short_commit: String,
    pub detached: bool,
}

impl GitHead {
    pub fn display(&self) -> String {
        if self.detached || self.branch.as_deref().unwrap_or_default().is_empty() {
            format!("@{}", self.short_commit)
        } else {
            self.branch.clone().unwrap_or_default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitWorktreeItem {
    pub path: PathBuf,
    pub head: GitHead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileStatusKind {
    Modified,
    Added,
    Untracked,
    Conflict,
}

impl GitFileStatusKind {
    pub fn marker(self) -> char {
        match self {
            Self::Modified => 'M',
            Self::Added => 'A',
            Self::Untracked => '?',
            Self::Conflict => 'U',
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GitFileStatus {
    pub index: Option<GitFileStatusKind>,
    pub worktree: Option<GitFileStatusKind>,
}

impl GitFileStatus {
    pub fn primary_kind(&self) -> Option<GitFileStatusKind> {
        if self.index == Some(GitFileStatusKind::Conflict)
            || self.worktree == Some(GitFileStatusKind::Conflict)
        {
            return Some(GitFileStatusKind::Conflict);
        }
        if self.index == Some(GitFileStatusKind::Untracked)
            || self.worktree == Some(GitFileStatusKind::Untracked)
        {
            return Some(GitFileStatusKind::Untracked);
        }
        if self.index == Some(GitFileStatusKind::Added)
            || self.worktree == Some(GitFileStatusKind::Added)
        {
            return Some(GitFileStatusKind::Added);
        }
        if self.index == Some(GitFileStatusKind::Modified)
            || self.worktree == Some(GitFileStatusKind::Modified)
        {
            return Some(GitFileStatusKind::Modified);
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitGutterMarkKind {
    Added,
    Modified,
}

impl GitGutterMarkKind {
    pub fn marker(self) -> char {
        match self {
            Self::Added => '+',
            Self::Modified => '~',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitGutterMarkRange {
    pub start_line: usize,
    pub end_line_exclusive: usize,
    pub kind: GitGutterMarkKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitGutterMarks {
    pub ranges: Vec<GitGutterMarkRange>,
    pub deletions: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct GitState {
    pub repo_root: Option<PathBuf>,
    pub head: Option<GitHead>,
    pub worktrees: Vec<GitWorktreeItem>,
    pub branches: Vec<String>,
    pub file_status: FxHashMap<PathBuf, GitFileStatus>,
}
