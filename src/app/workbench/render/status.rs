use super::super::Workbench;
use crate::kernel::editor::DiskState;
use crate::kernel::FocusTarget;
use crate::ui::core::geom::{Pos, Rect as UiRect};
use crate::ui::core::painter::Painter;
use crate::ui::core::style::{Mod, Style as UiStyle};
use unicode_width::UnicodeWidthStr;

impl Workbench {
    fn focus_label(&self) -> &'static str {
        match self.store.state().ui.focus {
            FocusTarget::Explorer => "explorer",
            FocusTarget::Editor => "editor",
            FocusTarget::Overlay => "overlay",
            FocusTarget::CommandLine => "command",
        }
    }

    /// demo 风格状态栏：左侧模式块 + `focus · 文件名` + 右侧 `行:列`。
    /// 模式块暂为静态 INSERT（绿）——模态编辑落地后变活。命令行激活时整条状态栏
    /// 由 `:` 命令行覆盖（见 layout），与 demo 的「命令模式状态栏变命令行」一致。
    pub(super) fn paint_status(&self, painter: &mut Painter, area: UiRect) {
        if area.is_empty() {
            return;
        }

        let active_pane = self.store.state().ui.editor_layout.active_pane;
        let (file_name, dirty, disk_indicator, cursor) = self
            .store
            .state()
            .editor
            .pane(active_pane)
            .and_then(|pane| pane.active_tab())
            .map(|tab| {
                let (row, col) = tab.buffer.cursor();
                let dirty = if tab.dirty { " [+]" } else { "" };
                let disk = match &tab.disk_state {
                    DiskState::ConflictExternalModified => " [CONFLICT]",
                    DiskState::MissingOnDisk => " [DELETED]",
                    DiskState::ReloadedFromDisk { .. } => " [RELOADED]",
                    DiskState::InSync => "",
                };
                let name = tab
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| tab.title.clone());
                (name, dirty, disk, Some((row, col)))
            })
            .unwrap_or_else(|| ("No file".to_string(), "", "", None));

        let base = UiStyle::default()
            .bg(self.theme.core.statusbar_bg)
            .fg(self.theme.core.palette_fg);
        painter.fill_rect(area, base);

        // 左：模式块。
        let chip = " INSERT ";
        let chip_style = UiStyle::default()
            .bg(self.theme.core.mode_insert_bg)
            .fg(self.theme.core.mode_text_fg)
            .add_mod(Mod::BOLD);
        painter.text_clipped(Pos::new(area.x, area.y), chip, chip_style, area);
        let x = area
            .x
            .saturating_add(chip.width().min(u16::MAX as usize) as u16);

        // 中：focus · 文件名。
        let mid = format!(
            "  {}  ·  {}{}{} ",
            self.focus_label(),
            file_name,
            dirty,
            disk_indicator
        );
        painter.text_clipped(Pos::new(x, area.y), mid.as_str(), base, area);

        // 右：行:列。
        if let Some((row, col)) = cursor {
            let right = format!(" {}:{} ", row + 1, col + 1);
            let right_w = right.width().min(u16::MAX as usize) as u16;
            let rx = area.right().saturating_sub(right_w);
            let muted = UiStyle::default()
                .bg(self.theme.core.statusbar_bg)
                .fg(self.theme.core.palette_muted_fg);
            painter.text_clipped(Pos::new(rx, area.y), right.as_str(), muted, area);
        }
    }
}
