use crate::models::{EditHistory, Granularity, TextBuffer};
use crate::kernel::services::ports::{EditorConfig, Match};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchBarMode {
    Search,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchBarField {
    Search,
    Replace,
}

#[derive(Debug)]
pub struct SearchBarState {
    pub visible: bool,
    pub mode: SearchBarMode,
    pub focused_field: SearchBarField,
    pub search_text: String,
    pub replace_text: String,
    pub cursor_pos: usize,
    pub case_sensitive: bool,
    pub use_regex: bool,
    pub matches: Vec<Match>,
    pub current_match_index: Option<usize>,
    pub searching: bool,
    pub active_search_id: Option<u64>,
    pub last_error: Option<String>,
}

impl Default for SearchBarState {
    fn default() -> Self {
        Self {
            visible: false,
            mode: SearchBarMode::Search,
            focused_field: SearchBarField::Search,
            search_text: String::new(),
            replace_text: String::new(),
            cursor_pos: 0,
            case_sensitive: false,
            use_regex: false,
            matches: Vec::new(),
            current_match_index: None,
            searching: false,
            active_search_id: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EditorViewportState {
    pub line_offset: usize,
    pub height: usize,
    pub horiz_offset: u32,
    pub width: usize,
    pub follow_cursor: bool,
}

impl Default for EditorViewportState {
    fn default() -> Self {
        Self {
            line_offset: 0,
            height: 20,
            horiz_offset: 0,
            width: 80,
            follow_cursor: true,
        }
    }
}

#[derive(Debug)]
pub struct EditorMouseState {
    pub last_click: Option<(u16, u16, Instant)>,
    pub click_count: u8,
    pub dragging: bool,
    pub granularity: Granularity,
}

impl EditorMouseState {
    pub fn new() -> Self {
        Self {
            last_click: None,
            click_count: 0,
            dragging: false,
            granularity: Granularity::Char,
        }
    }
}

impl Default for EditorMouseState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct EditorTabState {
    pub title: String,
    pub path: Option<PathBuf>,
    pub buffer: TextBuffer,
    pub viewport: EditorViewportState,
    pub history: EditHistory,
    pub dirty: bool,
    pub mouse: EditorMouseState,
}

impl std::fmt::Debug for EditorTabState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EditorTabState")
            .field("title", &self.title)
            .field("path", &self.path)
            .field("dirty", &self.dirty)
            .field("cursor", &self.buffer.cursor())
            .field("lines", &self.buffer.len_lines())
            .finish()
    }
}

impl EditorTabState {
    pub fn untitled(config: &EditorConfig) -> Self {
        let buffer = TextBuffer::new();
        let history = EditHistory::new(buffer.rope().clone());
        Self {
            title: "Untitled".to_string(),
            path: None,
            buffer,
            viewport: EditorViewportState {
                height: config.default_viewport_height,
                ..EditorViewportState::default()
            },
            history,
            dirty: false,
            mouse: EditorMouseState::new(),
        }
    }

    pub fn from_file(path: PathBuf, content: &str, config: &EditorConfig) -> Self {
        let title = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        let buffer = TextBuffer::from_text(content);
        let history = EditHistory::new(buffer.rope().clone());

        Self {
            title,
            path: Some(path),
            buffer,
            viewport: EditorViewportState {
                height: config.default_viewport_height,
                ..EditorViewportState::default()
            },
            history,
            dirty: false,
            mouse: EditorMouseState::new(),
        }
    }

    pub fn display_title(&self) -> String {
        if self.dirty {
            format!("● {}", self.title)
        } else {
            self.title.clone()
        }
    }
}

#[derive(Debug)]
pub struct EditorPaneState {
    pub tabs: Vec<EditorTabState>,
    pub active: usize,
    pub search_bar: SearchBarState,
}

impl EditorPaneState {
    pub fn new(config: &EditorConfig) -> Self {
        Self {
            tabs: vec![EditorTabState::untitled(config)],
            active: 0,
            search_bar: SearchBarState::default(),
        }
    }

    pub fn active_tab(&self) -> Option<&EditorTabState> {
        self.tabs.get(self.active)
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut EditorTabState> {
        self.tabs.get_mut(self.active)
    }

    pub fn set_active(&mut self, index: usize) -> bool {
        let index = index.min(self.tabs.len().saturating_sub(1));
        if index == self.active {
            return false;
        }
        self.active = index;
        true
    }

    pub fn open_file(&mut self, path: PathBuf, content: &str, config: &EditorConfig) -> bool {
        for (i, tab) in self.tabs.iter().enumerate() {
            if tab.path.as_ref() == Some(&path) {
                return self.set_active(i);
            }
        }

        self.tabs.push(EditorTabState::from_file(path, content, config));
        self.active = self.tabs.len().saturating_sub(1);
        true
    }

    pub fn close_active_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }

        let index = self.active.min(self.tabs.len().saturating_sub(1));
        self.tabs.remove(index);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        }
        true
    }

    pub fn next_tab(&mut self) -> bool {
        let len = self.tabs.len();
        if len <= 1 {
            return false;
        }
        let prev = self.active;
        self.active = (self.active + 1) % len;
        self.active != prev
    }

    pub fn prev_tab(&mut self) -> bool {
        let len = self.tabs.len();
        if len <= 1 {
            return false;
        }
        let prev = self.active;
        self.active = if self.active == 0 { len - 1 } else { self.active - 1 };
        self.active != prev
    }

    pub fn set_viewport_size(&mut self, width: usize, height: usize) -> bool {
        let width = width.max(1);
        let height = height.max(1);

        let mut changed = false;
        for tab in &mut self.tabs {
            if tab.viewport.width != width {
                tab.viewport.width = width;
                changed = true;
            }
            if tab.viewport.height != height {
                tab.viewport.height = height;
                changed = true;
            }
        }
        changed
    }
}

#[derive(Debug)]
pub struct EditorState {
    pub config: EditorConfig,
    pub panes: Vec<EditorPaneState>,
}

impl EditorState {
    pub fn new(config: EditorConfig) -> Self {
        Self {
            config: config.clone(),
            panes: vec![EditorPaneState::new(&config)],
        }
    }

    pub fn pane_mut(&mut self, pane: usize) -> Option<&mut EditorPaneState> {
        self.panes.get_mut(pane)
    }

    pub fn pane(&self, pane: usize) -> Option<&EditorPaneState> {
        self.panes.get(pane)
    }

    pub fn ensure_panes(&mut self, desired: usize) -> bool {
        let desired = desired.max(1);
        let current = self.panes.len();
        match desired.cmp(&current) {
            std::cmp::Ordering::Equal => false,
            std::cmp::Ordering::Less => {
                self.panes.truncate(desired);
                true
            }
            std::cmp::Ordering::Greater => {
                self.panes.reserve(desired - current);
                for _ in current..desired {
                    self.panes.push(EditorPaneState::new(&self.config));
                }
                true
            }
        }
    }
}
