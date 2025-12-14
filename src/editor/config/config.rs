//zcode/src/editor/config.rs

/// 编辑器配置
#[derive(Clone, Debug)]
pub struct EditorConfig {
    /// Tab 大小
    pub tab_size: u8,
    
    /// 默认视口高度
    pub default_viewport_height: usize,
    
    /// 双击时间阈值（毫秒）
    pub double_click_ms: u64,
    
    /// 三击时间阈值（毫秒）
    pub triple_click_ms: u64,
    
    /// 点击位置容差（像素）
    pub click_slop: u16,
    
    /// 滚轮滚动步长因子（viewport_height 的倍数）
    pub scroll_step_factor: f32,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            tab_size: 4,
            default_viewport_height: 20,
            double_click_ms: 300,
            triple_click_ms: 450,
            click_slop: 2,
            scroll_step_factor: 0.16, // ~1/6
        }
    }
}

impl EditorConfig {
    /// 计算动态滚动步长
    pub fn scroll_step(&self, viewport_height: usize) -> usize {
        ((viewport_height as f32 * self.scroll_step_factor) as usize).max(1)
    }
}
