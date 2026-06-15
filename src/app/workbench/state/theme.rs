//! 主题状态：源主题（`source`）与其派生的 core 主题（`core`）始终保持一致。
//! `core` 由 `source` 经 `to_core_theme` + `adapt_theme` 派生，不可独立设置，
//! 避免两份副本被分别赋值而失同步。

use crate::app::theme::UiTheme;
use crate::ui::core::color_support::TerminalColorSupport;
use crate::ui::core::theme::Theme;

pub(in crate::app::workbench) struct ThemeState {
    pub(in crate::app::workbench) source: UiTheme,
    pub(in crate::app::workbench) core: Theme,
    pub(in crate::app::workbench) color_support: TerminalColorSupport,
}

impl ThemeState {
    pub(in crate::app::workbench) fn new(
        source: UiTheme,
        color_support: TerminalColorSupport,
    ) -> Self {
        let core = Self::derive_core(&source, color_support);
        Self {
            source,
            core,
            color_support,
        }
    }

    /// 用新的源主题整体替换：core 随之重新派生，三者一次性同步更新。
    pub(in crate::app::workbench) fn set(
        &mut self,
        source: UiTheme,
        color_support: TerminalColorSupport,
    ) {
        self.core = Self::derive_core(&source, color_support);
        self.source = source;
        self.color_support = color_support;
    }

    /// 在直接修改 `source` 字段后调用：用当前 source + color_support 重新派生 core。
    pub(in crate::app::workbench) fn refresh_core(&mut self) {
        self.core = Self::derive_core(&self.source, self.color_support);
    }

    fn derive_core(source: &UiTheme, color_support: TerminalColorSupport) -> Theme {
        let core_theme = crate::app::theme::to_core_theme(source);
        crate::ui::core::theme_adapter::adapt_theme(&core_theme, color_support)
    }
}
