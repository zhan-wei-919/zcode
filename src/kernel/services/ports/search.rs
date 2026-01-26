use std::io;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, SearchError>;

#[derive(Debug)]
pub enum SearchError {
    Io(io::Error),
    InvalidRegex(regex::Error),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::Io(e) => write!(f, "IO error: {}", e),
            SearchError::InvalidRegex(e) => write!(f, "Invalid regex: {}", e),
        }
    }
}

impl std::error::Error for SearchError {}

impl From<io::Error> for SearchError {
    fn from(e: io::Error) -> Self {
        SearchError::Io(e)
    }
}

impl From<regex::Error> for SearchError {
    fn from(e: regex::Error) -> Self {
        SearchError::InvalidRegex(e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

impl Match {
    pub fn new(start: usize, end: usize, line: usize, col: usize) -> Self {
        Self {
            start,
            end,
            line,
            col,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SearchMessage {
    Matches {
        search_id: u64,
        matches: Vec<Match>,
        is_final: bool,
    },
    Complete {
        search_id: u64,
        total: usize,
    },
    Cancelled {
        search_id: u64,
    },
    Error {
        search_id: u64,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct FileMatches {
    pub path: PathBuf,
    pub matches: Vec<Match>,
}

#[derive(Debug, Clone)]
pub enum GlobalSearchMessage {
    FileMatches {
        search_id: u64,
        file_matches: FileMatches,
    },
    Progress {
        search_id: u64,
        files_searched: usize,
        files_with_matches: usize,
    },
    Complete {
        search_id: u64,
        total_files: usize,
        total_matches: usize,
    },
    Cancelled {
        search_id: u64,
    },
    Error {
        search_id: u64,
        message: String,
    },
}
