use std::path::PathBuf;

pub type TerminalId = u64;

const DEFAULT_SCROLLBACK_LINES: usize = 5000;

#[cfg(feature = "terminal")]
use vt100;

pub struct TerminalParser {
    #[cfg(feature = "terminal")]
    inner: vt100::Parser,
}

impl TerminalParser {
    pub fn new(rows: u16, cols: u16, scrollback_len: usize) -> Self {
        #[cfg(feature = "terminal")]
        {
            return Self {
                inner: vt100::Parser::new(rows, cols, scrollback_len),
            };
        }

        #[cfg(not(feature = "terminal"))]
        {
            let _ = (rows, cols, scrollback_len);
            Self {}
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        #[cfg(feature = "terminal")]
        {
            self.inner.process(bytes);
        }

        #[cfg(not(feature = "terminal"))]
        {
            let _ = bytes;
        }
    }

    #[cfg(feature = "terminal")]
    pub fn screen(&self) -> &vt100::Screen {
        self.inner.screen()
    }

    #[cfg(feature = "terminal")]
    pub fn screen_mut(&mut self) -> &mut vt100::Screen {
        self.inner.screen_mut()
    }
}

impl Default for TerminalParser {
    fn default() -> Self {
        Self::new(24, 80, 0)
    }
}

impl std::fmt::Debug for TerminalParser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalParser").finish()
    }
}

#[derive(Debug)]
pub struct TerminalSession {
    pub id: TerminalId,
    pub title: String,
    pub cwd: PathBuf,
    pub cols: u16,
    pub rows: u16,
    pub scroll_offset: usize,
    pub parser: TerminalParser,
    pub dirty: bool,
    pub exited: bool,
    pub exit_code: Option<i32>,
}

impl TerminalSession {
    pub fn new(
        id: TerminalId,
        cwd: PathBuf,
        cols: u16,
        rows: u16,
        scrollback_lines: usize,
    ) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            id,
            title: format!("terminal-{id}"),
            cwd,
            cols,
            rows,
            scroll_offset: 0,
            parser: TerminalParser::new(rows, cols, scrollback_lines),
            dirty: true,
            exited: false,
            exit_code: None,
        }
    }

    pub fn process_output(&mut self, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return false;
        }

        self.parser.process(bytes);

        #[cfg(feature = "terminal")]
        {
            self.scroll_offset = self.parser.screen().scrollback();
        }

        self.dirty = true;
        true
    }

    pub fn resize(&mut self, cols: u16, rows: u16) -> bool {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if self.cols == cols && self.rows == rows {
            return false;
        }

        self.cols = cols;
        self.rows = rows;

        #[cfg(feature = "terminal")]
        {
            self.parser.screen_mut().set_size(rows, cols);
            self.parser.screen_mut().set_scrollback(self.scroll_offset);
            self.scroll_offset = self.parser.screen().scrollback();
        }

        self.dirty = true;
        true
    }

    pub fn scroll(&mut self, delta: isize) -> bool {
        if delta == 0 {
            return false;
        }

        #[cfg(feature = "terminal")]
        {
            let current = self.parser.screen().scrollback();
            let next = if delta > 0 {
                current.saturating_add(delta as usize)
            } else {
                current.saturating_sub((-delta) as usize)
            };
            self.parser.screen_mut().set_scrollback(next);
            let actual = self.parser.screen().scrollback();
            if actual == self.scroll_offset {
                return false;
            }
            self.scroll_offset = actual;
        }

        self.dirty = true;
        true
    }

    #[cfg(feature = "terminal")]
    pub fn visible_rows(&self, width: u16, height: u16) -> Vec<String> {
        if width == 0 || height == 0 {
            return Vec::new();
        }

        let mut rows = Vec::with_capacity(height as usize);
        for (idx, row) in self.parser.screen().rows(0, width).enumerate() {
            if idx >= height as usize {
                break;
            }
            rows.push(row);
        }

        while rows.len() < height as usize {
            rows.push(String::new());
        }

        rows
    }
}

#[derive(Debug)]
pub struct TerminalState {
    pub sessions: Vec<TerminalSession>,
    pub active: Option<TerminalId>,
    pub next_id: TerminalId,
    pub scrollback_lines: usize,
}

impl Default for TerminalState {
    fn default() -> Self {
        Self {
            sessions: Vec::new(),
            active: None,
            next_id: 1,
            scrollback_lines: DEFAULT_SCROLLBACK_LINES,
        }
    }
}

impl TerminalState {
    pub fn active_session(&self) -> Option<&TerminalSession> {
        let id = self.active?;
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn active_session_mut(&mut self) -> Option<&mut TerminalSession> {
        let id = self.active?;
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn session_mut(&mut self, id: TerminalId) -> Option<&mut TerminalSession> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    pub fn ensure_session(&mut self, cwd: PathBuf, cols: u16, rows: u16) -> Option<TerminalId> {
        if let Some(id) = self.active {
            if self.sessions.iter().any(|s| s.id == id) {
                return None;
            }
        }

        if let Some(existing) = self.sessions.first().map(|s| s.id) {
            self.active = Some(existing);
            return None;
        }

        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let session = TerminalSession::new(id, cwd, cols, rows, self.scrollback_lines);
        self.sessions.push(session);
        self.active = Some(id);
        Some(id)
    }

    pub fn remove_session(&mut self, id: TerminalId) -> bool {
        let before = self.sessions.len();
        self.sessions.retain(|s| s.id != id);
        if self.active == Some(id) {
            self.active = self.sessions.first().map(|s| s.id);
        }
        before != self.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cwd() -> PathBuf {
        std::env::temp_dir()
    }

    #[test]
    fn terminal_state_ensure_session_allocates_once() {
        let mut state = TerminalState::default();
        let cwd = temp_cwd();

        let created = state.ensure_session(cwd.clone(), 80, 24);
        assert_eq!(created, Some(1));
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.active, Some(1));

        let created_again = state.ensure_session(cwd, 80, 24);
        assert_eq!(created_again, None);
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.active, Some(1));
    }

    #[test]
    fn terminal_state_recovers_active_when_missing() {
        let mut state = TerminalState::default();
        let cwd = temp_cwd();
        let _ = state.ensure_session(cwd, 80, 24);

        state.active = Some(999);
        let created = state.ensure_session(temp_cwd(), 80, 24);
        assert_eq!(created, None);
        assert_eq!(state.active, Some(1));
    }

    #[test]
    fn terminal_session_resize_clamps_to_one() {
        let mut session = TerminalSession::new(1, temp_cwd(), 10, 5, 10);
        assert!(session.resize(0, 0));
        assert_eq!(session.cols, 1);
        assert_eq!(session.rows, 1);
        assert!(!session.resize(1, 1));
    }

    #[test]
    fn terminal_session_scroll_clamps_to_zero() {
        let mut session = TerminalSession::new(1, temp_cwd(), 10, 3, 10);
        let mut data = Vec::new();
        for idx in 0..10 {
            data.extend_from_slice(format!("line{idx}\n").as_bytes());
        }
        assert!(session.process_output(&data));

        assert!(session.scroll(2));
        assert_eq!(session.scroll_offset, 2);
        assert!(session.scroll(-999));
        assert_eq!(session.scroll_offset, 0);
    }

    #[test]
    fn terminal_session_marks_dirty_on_output() {
        let mut session = TerminalSession::new(1, temp_cwd(), 10, 3, 10);
        session.dirty = false;
        assert!(session.process_output(b"hello\n"));
        assert!(session.dirty);
    }
}
