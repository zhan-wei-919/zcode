//zcode/src/editor/input/mouse.rs
//! 鼠标控制器：处理点击计数、拖拽状态

use std::time::{Duration, Instant};
use super::selection::Granularity;
use crate::editor::config::EditorConfig;

/// 鼠标控制器
pub struct MouseController {
    /// 上次按下的位置和时间
    last_down: Option<(u16, u16, Instant)>,
    
    /// 连续点击计数
    click_count: u8,
    
    /// 是否正在拖拽
    dragging: bool,
    
    /// 当前粒度
    granularity: Granularity,
    
    /// 配置（包含时间阈值等）
    config: EditorConfig,
}

impl MouseController {
    /// 创建新的鼠标控制器（使用默认配置）
    pub fn new() -> Self {
        Self::with_config(EditorConfig::default())
    }
    
    /// 创建新的鼠标控制器（使用自定义配置）
    pub fn with_config(config: EditorConfig) -> Self {
        Self {
            last_down: None,
            click_count: 0,
            dragging: false,
            granularity: Granularity::Char,
            config,
        }
    }
    
    /// 处理鼠标按下事件
    /// 返回：当前的选择粒度
    pub fn on_mouse_down(&mut self, x: u16, y: u16, now: Instant) -> Granularity {
        // 检查是否是连续点击
        if let Some((last_x, last_y, last_time)) = self.last_down {
            let elapsed = now.duration_since(last_time);
            let distance_ok = x.abs_diff(last_x) <= self.config.click_slop 
                           && y.abs_diff(last_y) <= self.config.click_slop;
            
            if distance_ok {
                // 判断是双击还是三击（使用配置的阈值）
                let double_threshold = Duration::from_millis(self.config.double_click_ms);
                let triple_threshold = Duration::from_millis(self.config.triple_click_ms);
                
                if elapsed <= double_threshold && self.click_count == 1 {
                    self.click_count = 2;
                } else if elapsed <= triple_threshold && self.click_count == 2 {
                    self.click_count = 3;
                } else {
                    self.click_count = 1;
                }
            } else {
                // 位置差太大，重置
                self.click_count = 1;
            }
        } else {
            // 第一次点击
            self.click_count = 1;
        }
        
        // 更新状态
        self.last_down = Some((x, y, now));
        self.dragging = true;
        
        // 根据点击次数确定粒度
        self.granularity = match self.click_count {
            1 => Granularity::Char,
            2 => Granularity::Word,
            _ => Granularity::Line,
        };
        
        self.granularity
    }
    
    /// 处理鼠标拖拽事件
    pub fn on_mouse_drag(&self) -> bool {
        self.dragging
    }
    
    /// 处理鼠标释放事件
    pub fn on_mouse_up(&mut self) {
        self.dragging = false;
    }
    
    /// 获取当前粒度
    pub fn granularity(&self) -> Granularity {
        self.granularity
    }
    
    /// 检查是否正在拖拽
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_single_click() {
        let mut ctrl = MouseController::new();
        let now = Instant::now();
        
        let gran = ctrl.on_mouse_down(10, 10, now);
        assert_eq!(gran, Granularity::Char);
        assert_eq!(ctrl.click_count, 1);
    }
    
    #[test]
    fn test_double_click() {
        let mut ctrl = MouseController::new();
        let now = Instant::now();
        
        // 第一次点击
        ctrl.on_mouse_down(10, 10, now);
        
        // 100ms 后第二次点击（同一位置）
        let gran = ctrl.on_mouse_down(10, 10, now + Duration::from_millis(100));
        assert_eq!(gran, Granularity::Word);
        assert_eq!(ctrl.click_count, 2);
    }
    
    #[test]
    fn test_triple_click() {
        let mut ctrl = MouseController::new();
        let now = Instant::now();
        
        // 三次快速点击
        ctrl.on_mouse_down(10, 10, now);
        ctrl.on_mouse_down(10, 10, now + Duration::from_millis(100));
        let gran = ctrl.on_mouse_down(10, 10, now + Duration::from_millis(200));
        
        assert_eq!(gran, Granularity::Line);
        assert_eq!(ctrl.click_count, 3);
    }
    
    #[test]
    fn test_click_timeout() {
        let mut ctrl = MouseController::new();
        let now = Instant::now();
        
        // 第一次点击
        ctrl.on_mouse_down(10, 10, now);
        
        // 500ms 后点击（超时）
        let gran = ctrl.on_mouse_down(10, 10, now + Duration::from_millis(500));
        assert_eq!(gran, Granularity::Char);
        assert_eq!(ctrl.click_count, 1);
    }
    
    #[test]
    fn test_click_distance() {
        let mut ctrl = MouseController::new();
        let now = Instant::now();
        
        // 第一次点击
        ctrl.on_mouse_down(10, 10, now);
        
        // 位置差太大
        let gran = ctrl.on_mouse_down(20, 20, now + Duration::from_millis(100));
        assert_eq!(gran, Granularity::Char);
        assert_eq!(ctrl.click_count, 1);
    }
}
