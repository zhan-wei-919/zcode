//zcode/src/editor/core/state.rs
//! 编辑器状态：组合 TextModel（数据） 和 EditorView（视图）
//! 
//! 这是一个轻量级的协调层，未来可以进一步拆分为命令模式

use super::text_model::TextModel;
use super::view::EditorView;
use crate::editor::input::{MouseController, Keybindings};
use crate::editor::config::EditorConfig;

/// 编辑器：组合数据模型和视图
/// 
/// 架构：
/// - TextModel: 文本数据、光标、选区（纯数据，无渲染逻辑）
/// - EditorView: 视口、布局、滚动（纯视图，无业务逻辑）
/// - MouseController: 鼠标交互状态
/// - Keybindings: 按键绑定系统
/// - EditorConfig: 配置参数（唯一事实来源）
/// 
/// 未来扩展：
/// - CommandHistory: 撤销/重做
/// - MultiCursor: 多光标支持
pub struct Editor {
    /// 文本模型（数据层）
    pub model: TextModel,
    
    /// 视图层（渲染层）
    pub view: EditorView,
    
    /// 鼠标控制器
    pub mouse_controller: MouseController,
    
    /// 键位绑定
    pub keybindings: Keybindings,
    
    /// 编辑器配置（唯一事实来源）
    pub config: EditorConfig,
}

impl Editor {
    /// 创建新编辑器（使用默认配置）
    pub fn new() -> Self {
        let config = EditorConfig::default();
        Self::with_config(config)
    }
    
    /// 使用自定义配置创建编辑器
    pub fn with_config(config: EditorConfig) -> Self {
        Self {
            model: TextModel::new(),
            view: EditorView::new(config.tab_size),
            mouse_controller: MouseController::with_config(config.clone()),
            keybindings: Keybindings::default(),
            config,
        }
    }
    
    // ==================== 便捷方法（代理到 model/view） ====================
    
    /// 获取光标位置
    pub fn cursor(&self) -> (usize, usize) {
        self.model.cursor()
    }
    
    /// 设置光标位置
    pub fn set_cursor(&mut self, row: usize, col: usize) {
        self.model.set_cursor(row, col);
    }
    
    /// 获取视口偏移
    pub fn viewport_offset(&self) -> usize {
        self.view.viewport_offset()
    }
    
    /// 获取视口高度
    pub fn viewport_height(&self) -> usize {
        self.view.viewport_height()
    }
    
    /// 更新视口状态
    pub fn update_viewport(&mut self, viewport_height: usize, viewport_width: usize) {
        self.view.update_viewport(&self.model, viewport_height, viewport_width);
    }
    
    /// 获取光标显示 x 坐标
    pub fn get_cursor_display_x(&mut self) -> u16 {
        self.view.get_cursor_display_x(&self.model)
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}