//! 主题状态：核心渲染主题 `core`，按终端色彩能力从默认主题派生。
//! 配色跟随终端，不做用户可定制，因此只持有派生结果与色彩能力。

use crate::ui::core::color_support::TerminalColorSupport;
use crate::ui::core::theme::Theme;

pub(in crate::app::workbench) struct ThemeState {
    pub(in crate::app::workbench) core: Theme,
}

impl ThemeState {
    pub(in crate::app::workbench) fn new(color_support: TerminalColorSupport) -> Self {
        Self {
            core: crate::ui::core::theme_adapter::adapt_theme(&Theme::default(), color_support),
        }
    }
}
