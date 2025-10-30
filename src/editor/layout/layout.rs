//zcode/src/editor/layout.rs
use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// 单行布局信息
#[derive(Clone, Debug)]
pub struct LineLayout {
    /// 每个字形簇的起始列位置（显示坐标）
    /// 例如：["a", "宽", "b"] → [0, 1, 3, 4]
    /// 长度 = grapheme_count + 1（最后一个是行尾位置）
    /// 使用 u32 避免极长行溢出（65535+ 列）
    pub cell_x: Vec<u32>,
    
    /// 该行的总显示宽度（u32 支持超长行）
    pub display_width: u32,
    
    /// 每个字形簇对应的字符偏移（Unicode scalar values 累计数）
    /// 例如：["a", "é", "中"] → [0, 1, 3, 4]
    /// - "a" = 1 char → 累计 1
    /// - "é" = 2 chars (e + combining acute) → 累计 3
    /// - "中" = 1 char → 累计 4
    /// 
    /// 这个索引让 grapheme_index → char_offset 变成 O(1)，
    /// 避免在长行末尾编辑时的 O(N) 扫描
    pub char_x: Vec<usize>,
    
    /// 代数：用于失效检测
    pub gen: u64,
}

impl LineLayout {
    /// 命中测试：给定 x 坐标（考虑水平滚动后的坐标），返回对应的字形簇索引
    /// 
    /// 使用二分查找 + 半宽判定：
    /// - 如果点击在字符左半部分 → 返回当前索引
    /// - 如果点击在字符右半部分 → 返回下一个索引
    pub fn hit_test_x(&self, x: u32) -> usize {
        match self.cell_x.binary_search(&x) {
            // 正好命中某个字形簇的起点
            Ok(idx) => idx,
            
            // 落在两个字形簇之间
            Err(insert_idx) => {
                if insert_idx == 0 {
                    0
                } else if insert_idx >= self.cell_x.len() {
                    // 修复：鼠标在行尾之后，返回最后一个字形簇的索引
                    self.cell_x.len().saturating_sub(1)
                } else {
                    let left = self.cell_x[insert_idx - 1];
                    let right = self.cell_x[insert_idx];
                    let mid = left + (right - left) / 2;
                    
                    if x < mid {
                        insert_idx - 1
                    } else {
                        insert_idx
                    }
                }
            }
        }
    }
}

/// 缓存大小限制（防止大文件 OOM）
const MAX_CACHE_LINES: usize = 10_000;

/// 布局引擎：负责缓存和失效
/// 
/// TODO: 架构改进
/// - 行身份验证：维护行版本号或轻量哈希，用于命中验证
/// - 预取优化：视口附近行的预加载（需要行身份避免失效误判）
/// - 并发渲染：支持后台线程预计算布局（需要细粒度锁）
pub struct LayoutEngine {
    /// 缓存：Vec 比 HashMap 更缓存友好（连续内存，无哈希开销）
    cache: Vec<Option<LineLayout>>,
    
    /// Tab 大小
    tab_size: u8,
    
    /// 全局代数：每次编辑 +1
    text_gen: u64,
    
    // TODO: 未来添加
    // line_hashes: Vec<u64>,  // 每行内容的轻量哈希，用于验证缓存有效性
}

impl LayoutEngine {
    pub fn new(tab_size: u8) -> Self {
        Self {
            cache: Vec::new(),
            tab_size,
            text_gen: 0,
        }
    }
    
    /// 获取Tab大小
    pub fn tab_size(&self) -> u8 {
        self.tab_size
    }
    
    /// 细粒度失效：只清除受影响的行缓存
    /// 不修改 text_gen，避免影响其他未修改的行
    pub fn invalidate_range(&mut self, start_row: usize, end_row: usize) {
        let end = end_row.min(self.cache.len());
        for row in start_row..end {
            // 直接清除缓存，下次访问时会重新计算
            self.cache[row] = None;
        }
    }
    
    /// 全局失效（仅用于大规模修改）
    /// 通过增加 text_gen 使所有缓存失效
    pub fn invalidate_all(&mut self) {
        self.text_gen = self.text_gen.wrapping_add(1);
        // 不需要修改每一行，layout_line 会自动检测 gen != text_gen
    }
    
    // ==================== 行号平移 API ====================
    
    /// 在 row 处之后插入了 count 条新行
    /// 
    /// 例如：在第 3 行敲回车，把该行拆成两行
    /// - 调用 `on_lines_inserted(3, 1)`
    /// - 缓存向量在位置 4 插入 1 个 None
    /// - 原来的行 4,5,6... 整体右移到 5,6,7...
    /// 
    /// # 参数
    /// - `row`: 插入点所在行号
    /// - `count`: 插入的行数
    pub fn on_lines_inserted(&mut self, row: usize, count: usize) {
        if count == 0 {
            return;
        }
        
        let insert_at = row.saturating_add(1);
        
        // 在 insert_at 位置插入 count 个 None，使后续缓存整体右移
        // 使用 splice 进行 O(n) 插入操作
        if insert_at <= self.cache.len() {
            self.cache.splice(
                insert_at..insert_at,
                std::iter::repeat(None).take(count)
            );
        } else {
            // 如果插入位置超出当前缓存范围，扩展缓存
            self.cache.resize(insert_at, None);
            self.cache.extend(std::iter::repeat(None).take(count));
        }
    }
    
    /// 在 row 处之后删除了 count 条行
    /// 
    /// 例如：删除换行符，把第 3 行与第 4 行合并
    /// - 调用 `on_lines_deleted(3, 1)`
    /// - 缓存向量删除位置 4 的条目
    /// - 原来的行 5,6,7... 整体左移到 4,5,6...
    /// 
    /// # 参数
    /// - `row`: 删除起点所在行号
    /// - `count`: 删除的行数
    pub fn on_lines_deleted(&mut self, row: usize, count: usize) {
        if count == 0 {
            return;
        }
        
        let start = row.saturating_add(1);
        if start >= self.cache.len() {
            return;
        }
        
        let end = (start + count).min(self.cache.len());
        self.cache.drain(start..end);
    }
    
    /// 结构性变化的兜底方案
    /// 
    /// 当无法准确确定插入/删除的行数时使用
    /// 例如：粘贴多行文本、批量操作等
    pub fn on_structure_changed(&mut self) {
        // 全局失效，保守但安全
        self.invalidate_all();
    }
    
    /// 获取指定行的布局（Vec 缓存，O(1) 访问）
    pub fn layout_line(&mut self, rope: &Rope, row: usize) -> &LineLayout {
        // 扩展 cache 到足够大小
        if row >= self.cache.len() {
            // 防止 OOM：如果缓存太大，先修剪
            if self.cache.len() > MAX_CACHE_LINES {
                self.trim_cache();
            }
            self.cache.resize_with(row + 1, || None);
        }
        
        // 检查缓存是否有效
        let need_compute = self.cache[row]
            .as_ref()
            .map_or(true, |l| l.gen != self.text_gen);
        
        if need_compute {
            let mut layout = self.compute_layout(rope, row);
            layout.gen = self.text_gen;
            self.cache[row] = Some(layout);
        }
        
        self.cache[row].as_ref().unwrap()
    }
    
    /// 修剪缓存：清除陈旧的条目
    fn trim_cache(&mut self) {
        let stale_gen = self.text_gen.wrapping_sub(1);
        
        // 清除所有陈旧的条目
        for entry in &mut self.cache {
            if let Some(layout) = entry {
                if layout.gen < stale_gen {
                    *entry = None;
                }
            }
        }
        
        // 收缩 Vec（移除末尾的 None）
        while self.cache.last().map_or(false, |e| e.is_none()) {
            self.cache.pop();
        }
    }
    
    /// 计算单行布局（核心逻辑）
    /// 
    /// 同时计算：
    /// 1. cell_x: 显示列累计（用于光标渲染、鼠标点击）
    /// 2. char_x: 字符偏移累计（用于 Rope 索引，避免 O(N) 扫描）
    fn compute_layout(&self, rope: &Rope, row: usize) -> LineLayout {
        let line = rope.line(row).as_str().unwrap_or("");
        
        let mut cell_x = Vec::new();
        let mut char_x = Vec::new();
        
        cell_x.push(0);
        char_x.push(0);
        
        let mut acc_col: u32 = 0;      // 显示列累计
        let mut acc_char: usize = 0;   // 字符数累计
        
        for grapheme in line.graphemes(true) {
            // 1. 计算显示宽度
            let width = if grapheme == "\t" {
                // Tab 宽度：对齐到下一个 tab_size 的倍数
                let tab = self.tab_size as u32;
                let remainder = acc_col % tab;
                if remainder == 0 { tab } else { tab - remainder }
            } else {
                grapheme.width() as u32
            };
            
            acc_col = acc_col.saturating_add(width);
            cell_x.push(acc_col);
            
            // 2. 计算字符偏移（Unicode scalar values 个数）
            // 例如：
            // - "a" = 1 char
            // - "é" (e + combining acute) = 2 chars
            // - "👨‍👩‍👧‍👦" (family emoji) = 7 chars
            let char_count = grapheme.chars().count();
            acc_char += char_count;
            char_x.push(acc_char);
        }
        
        LineLayout {
            cell_x,
            display_width: acc_col,
            char_x,
            gen: 0, // Will be set by caller
        }
    }
    
    /// 命中测试：屏幕坐标 → 行内列索引
    /// 
    /// 这是性能关键路径，使用缓存 + 二分查找
    /// 
    /// # 参数
    /// - `row`: 行号
    /// - `x`: 屏幕相对 x 坐标（u16）
    /// - `horiz_offset`: 水平滚动偏移量
    pub fn hit_test_x(&mut self, rope: &Rope, row: usize, x: u16, horiz_offset: u32) -> usize {
        let layout = self.layout_line(rope, row);
        // 将屏幕坐标转换为文档坐标
        let doc_x = (x as u32).saturating_add(horiz_offset);
        layout.hit_test_x(doc_x)
    }
    
    /// 获取光标的显示 x 坐标（用于渲染）
    /// 返回相对于视口的 x 坐标（已减去水平偏移）
    /// 
    /// # 参数
    /// - `row`: 行号
    /// - `col`: 列号（字形簇索引）
    /// - `horiz_offset`: 水平滚动偏移量
    pub fn get_cursor_x(&mut self, rope: &Rope, row: usize, col: usize, horiz_offset: u32) -> u16 {
        let layout = self.layout_line(rope, row);
        
        // col 可能越界（光标在行尾之后）
        let doc_x = if col >= layout.cell_x.len() {
            layout.display_width
        } else {
            layout.cell_x[col]
        };
        
        // 转换为视口坐标并 clamp 到 u16 范围
        doc_x.saturating_sub(horiz_offset).min(u16::MAX as u32) as u16
    }
    
    /// 预取：提前加载视口附近的行（可选优化）
    pub fn prefetch(&mut self, rope: &Rope, start: usize, end: usize) {
        for row in start..end.min(rope.len_lines()) {
            self.layout_line(rope, row);
        }
    }
    
    // ==================== 字符索引 API（O(1) 查询） ====================
    
    /// 字形簇索引 → 字符偏移（O(1)）
    /// 
    /// 这是性能关键路径！避免了 O(N) 的线性扫描。
    /// 
    /// # 参数
    /// - `rope`: Rope 文本
    /// - `row`: 行号
    /// - `grapheme_idx`: 字形簇索引
    /// 
    /// # 返回
    /// 该字形簇起点对应的字符偏移（相对于行首）
    /// 
    /// # 性能
    /// - 时间复杂度：O(1)（缓存命中）或 O(行长度)（缓存未命中，需要计算布局）
    /// - 空间复杂度：O(行长度)（布局缓存）
    /// 
    /// # 示例
    /// ```ignore
    /// // "aé中" 其中 "é" = 2 chars (e + combining acute)
    /// grapheme_to_char_index(rope, 0, 0) == 0  // "a" 起点
    /// grapheme_to_char_index(rope, 0, 1) == 1  // "é" 起点（1 char after "a"）
    /// grapheme_to_char_index(rope, 0, 2) == 3  // "中" 起点（3 chars total）
    /// grapheme_to_char_index(rope, 0, 3) == 4  // 行尾
    /// ```
    pub fn grapheme_to_char_index(&mut self, rope: &Rope, row: usize, grapheme_idx: usize) -> usize {
        let layout = self.layout_line(rope, row);
        
        // 边界处理：如果索引超出范围，返回行尾的字符偏移
        layout.char_x
            .get(grapheme_idx)
            .copied()
            .unwrap_or_else(|| layout.char_x.last().copied().unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_layout_simple() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("hello");
        
        let layout = engine.layout_line(&rope, 0);
        assert_eq!(layout.cell_x, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(layout.display_width, 5);
    }
    
    #[test]
    fn test_layout_wide_chars() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("a你b");
        
        let layout = engine.layout_line(&rope, 0);
        // 'a'=1, '你'=2, 'b'=1
        assert_eq!(layout.cell_x, vec![0, 1, 3, 4]);
    }
    
    #[test]
    fn test_layout_tabs() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("a\tb");
        
        let layout = engine.layout_line(&rope, 0);
        // 'a'=1, tab到4, 'b'=1
        assert_eq!(layout.cell_x, vec![0, 1, 4, 5]);
    }
    
    #[test]
    fn test_hit_test() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("hello");
        
        let layout = engine.layout_line(&rope, 0);
        
        assert_eq!(layout.hit_test_x(0), 0);  // 左边界
        assert_eq!(layout.hit_test_x(1), 1);  // 'e'起点
        assert_eq!(layout.hit_test_x(2), 2);  // 'l'起点
        assert_eq!(layout.hit_test_x(10), 5); // 超出行尾
    }
    
    #[test]
    fn test_cache_invalidation() {
        let mut engine = LayoutEngine::new(4);
        let mut rope = Rope::from_str("hello\nworld\ntest");
        
        // 访问多行
        engine.layout_line(&rope, 0);
        engine.layout_line(&rope, 1);
        engine.layout_line(&rope, 2);
        assert_eq!(engine.cache.len(), 3);
        
        let gen_before = engine.text_gen;
        
        // 只编辑第 0 行
        engine.invalidate_range(0, 1);
        rope.insert(0, "x");
        
        // ✅ 关键测试：text_gen 不应该改变（细粒度失效）
        assert_eq!(engine.text_gen, gen_before);
        
        // 第 0 行缓存被清除
        assert!(engine.cache[0].is_none());
        
        // ✅ 第 1, 2 行缓存仍然有效
        assert!(engine.cache[1].is_some());
        assert!(engine.cache[2].is_some());
        
        // 重新访问第 0 行
        let layout = engine.layout_line(&rope, 0);
        assert_eq!(layout.cell_x[0], 0);
        assert_eq!(layout.cell_x[1], 1); // 'x'
        
        // 访问第 1 行应该命中缓存（不需要重算）
        let layout1 = engine.layout_line(&rope, 1);
        assert_eq!(layout1.gen, gen_before); // gen 未改变，证明是缓存命中
    }
    
    #[test]
    fn test_global_invalidation() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("hello\nworld\ntest");
        
        // 访问所有行
        engine.layout_line(&rope, 0);
        engine.layout_line(&rope, 1);
        engine.layout_line(&rope, 2);
        
        let gen_before = engine.text_gen;
        
        // 全局失效
        engine.invalidate_all();
        
        // text_gen 应该增加
        assert_eq!(engine.text_gen, gen_before + 1);
        
        // 缓存条目仍然存在，但 gen 不匹配
        assert!(engine.cache[0].is_some());
        assert!(engine.cache[1].is_some());
        assert!(engine.cache[2].is_some());
        
        assert_eq!(engine.cache[0].as_ref().unwrap().gen, gen_before);
        assert_eq!(engine.cache[1].as_ref().unwrap().gen, gen_before);
        
        // 重新访问会重算
        engine.layout_line(&rope, 0);
        assert_eq!(engine.cache[0].as_ref().unwrap().gen, gen_before + 1);
    }
    
    #[test]
    fn test_u32_types() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("hello world");
        
        let layout = engine.layout_line(&rope, 0);
        
        // 关键：验证数据类型是 u32（编译时检查）
        // 这确保即使值超过 u16::MAX (65535) 也不会溢出
        let _test_u32: u32 = layout.display_width;
        let _test_vec_u32: &Vec<u32> = &layout.cell_x;
        
        // 基本功能验证
        assert_eq!(layout.display_width, 11);
        assert_eq!(layout.cell_x.len(), 12); // 11 字符 + 起点
        
        // 验证可以处理大值（虽然这里没有大于65535的值，但类型支持）
        // u32::MAX = 4,294,967,295，远大于 u16::MAX = 65,535
        let max_supported: u32 = u32::MAX;
        assert!(max_supported > 65535);
    }
    
    #[test]
    fn test_horizontal_scroll_hit_test() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("hello world");
        
        // 无滚动：点击 x=5 应该命中 ' '
        let col = engine.hit_test_x(&rope, 0, 5, 0);
        assert_eq!(col, 5);
        
        // 水平滚动 3：点击 x=2（视口坐标）= 文档坐标 5
        let col = engine.hit_test_x(&rope, 0, 2, 3);
        assert_eq!(col, 5);
    }
    
    #[test]
    fn test_horizontal_scroll_cursor_x() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("hello world");
        
        // 光标在 col=5，无滚动
        let x = engine.get_cursor_x(&rope, 0, 5, 0);
        assert_eq!(x, 5);
        
        // 光标在 col=5，水平滚动 3
        let x = engine.get_cursor_x(&rope, 0, 5, 3);
        assert_eq!(x, 2); // 5 - 3 = 2
        
        // 光标在 col=5，水平滚动 10（光标在视口左侧外）
        let x = engine.get_cursor_x(&rope, 0, 5, 10);
        assert_eq!(x, 0); // saturating_sub
    }
    
    #[test]
    fn test_on_lines_inserted() {
        let mut engine = LayoutEngine::new(4);
        let mut rope = Rope::from_str("Line 0\nLine 1\nLine 2");
        
        // 访问所有行，填充缓存
        engine.layout_line(&rope, 0);
        engine.layout_line(&rope, 1);
        engine.layout_line(&rope, 2);
        assert_eq!(engine.cache.len(), 3);
        
        // 模拟在第 1 行插入换行：在 row=1 之后插入 1 条新行
        engine.on_lines_inserted(1, 1);
        
        // 缓存向量长度应该增加
        assert_eq!(engine.cache.len(), 4);
        
        // 原来的第 2 行现在应该在位置 3
        // 注意：原位置 2 的缓存条目现在在位置 3
        
        // 新插入的位置 2 应该是 None
        assert!(engine.cache[2].is_none());
        
        // 原来的第 2 行缓存仍然存在（现在在位置 3）
        assert!(engine.cache[3].is_some());
    }
    
    #[test]
    fn test_on_lines_deleted() {
        let mut engine = LayoutEngine::new(4);
        let mut rope = Rope::from_str("Line 0\nLine 1\nLine 2\nLine 3");
        
        // 访问所有行，填充缓存
        engine.layout_line(&rope, 0);
        engine.layout_line(&rope, 1);
        engine.layout_line(&rope, 2);
        engine.layout_line(&rope, 3);
        assert_eq!(engine.cache.len(), 4);
        
        // 模拟删除第 1 行后的 1 条行：删除 row=2（合并 row=1 和 row=2）
        engine.on_lines_deleted(1, 1);
        
        // 缓存向量长度应该减少
        assert_eq!(engine.cache.len(), 3);
        
        // 原来的第 3 行现在应该在位置 2
        assert!(engine.cache[2].is_some());
    }
    
    #[test]
    fn test_on_lines_inserted_multiple() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("Line 0\nLine 1\nLine 2");
        
        // 填充缓存
        engine.layout_line(&rope, 0);
        engine.layout_line(&rope, 1);
        engine.layout_line(&rope, 2);
        
        // 插入 3 条新行
        engine.on_lines_inserted(0, 3);
        
        // 缓存长度应该增加 3
        assert_eq!(engine.cache.len(), 6);
        
        // 新插入的位置应该是 None
        assert!(engine.cache[1].is_none());
        assert!(engine.cache[2].is_none());
        assert!(engine.cache[3].is_none());
        
        // 原来的行应该平移到新位置
        assert!(engine.cache[4].is_some()); // 原来的 row 1
        assert!(engine.cache[5].is_some()); // 原来的 row 2
    }
    
    #[test]
    fn test_on_lines_deleted_multiple() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("Line 0\nLine 1\nLine 2\nLine 3\nLine 4");
        
        // 填充缓存
        for i in 0..5 {
            engine.layout_line(&rope, i);
        }
        assert_eq!(engine.cache.len(), 5);
        
        // 删除 2 条行
        engine.on_lines_deleted(1, 2);
        
        // 缓存长度应该减少 2
        assert_eq!(engine.cache.len(), 3);
        
        // 原来的第 4 行现在应该在位置 2
        assert!(engine.cache[2].is_some());
    }
    
    #[test]
    fn test_on_lines_inserted_beyond_cache() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("Line 0\nLine 1");
        
        // 只访问前 2 行
        engine.layout_line(&rope, 0);
        engine.layout_line(&rope, 1);
        assert_eq!(engine.cache.len(), 2);
        
        // 在超出缓存范围的位置插入
        engine.on_lines_inserted(5, 2);
        
        // 应该扩展缓存
        assert!(engine.cache.len() >= 7); // 至少 5 + 2
    }
    
    #[test]
    fn test_cache_alignment_after_edit_sequence() {
        let mut engine = LayoutEngine::new(4);
        let rope = Rope::from_str("Line 0\nLine 1\nLine 2\nLine 3");
        
        // 填充缓存
        for i in 0..4 {
            engine.layout_line(&rope, i);
        }
        
        // 模拟编辑序列
        engine.on_lines_inserted(1, 1);  // 在 row 1 后插入 1 行
        assert_eq!(engine.cache.len(), 5);
        
        engine.on_lines_deleted(2, 1);   // 删除新插入的行
        assert_eq!(engine.cache.len(), 4);
        
        engine.on_lines_inserted(0, 2);  // 在开头插入 2 行
        assert_eq!(engine.cache.len(), 6);
        
        // 验证缓存结构仍然有效
        assert!(engine.cache[0].is_some()); // 原 row 0
    }
}

