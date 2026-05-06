use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::drawable::*;

/// 绘制内容管理器（参考 BetterGI DrawContent）
///
/// 使用 key-based 管理，每个绘制元素有唯一 key，
/// 可单独更新/删除，支持线程安全访问。
#[derive(Clone)]
pub struct DrawContent {
    inner: Arc<RwLock<DrawContentInner>>,
}

struct DrawContentInner {
    rects: HashMap<String, Vec<RectDrawable>>,
    texts: HashMap<String, Vec<TextDrawable>>,
    lines: HashMap<String, Vec<LineDrawable>>,
    match_results: HashMap<String, Vec<MatchResultDrawable>>,
    crosshairs: HashMap<String, Vec<CrosshairDrawable>>,
    progress_bars: HashMap<String, Vec<ProgressBarDrawable>>,
    dirty: bool,
}

impl DrawContent {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(DrawContentInner {
                rects: HashMap::new(),
                texts: HashMap::new(),
                lines: HashMap::new(),
                match_results: HashMap::new(),
                crosshairs: HashMap::new(),
                progress_bars: HashMap::new(),
                dirty: false,
            })),
        }
    }

    /// 标记需要重绘
    fn mark_dirty(inner: &mut DrawContentInner) {
        inner.dirty = true;
    }

    /// 检查并清除脏标记
    pub fn take_dirty(&self) -> bool {
        let mut inner = self.inner.write().unwrap();
        let dirty = inner.dirty;
        inner.dirty = false;
        dirty
    }

    // ━━━ 矩形 ━━━

    pub fn put_rect(&self, key: impl Into<String>, rect: RectDrawable) {
        let mut inner = self.inner.write().unwrap();
        inner.rects.insert(key.into(), vec![rect]);
        Self::mark_dirty(&mut inner);
    }

    pub fn put_rect_list(&self, key: impl Into<String>, list: Vec<RectDrawable>) {
        let mut inner = self.inner.write().unwrap();
        if list.is_empty() {
            inner.rects.remove(&key.into());
        } else {
            inner.rects.insert(key.into(), list);
        }
        Self::mark_dirty(&mut inner);
    }

    pub fn remove_rect(&self, key: &str) {
        let mut inner = self.inner.write().unwrap();
        if inner.rects.remove(key).is_some() {
            Self::mark_dirty(&mut inner);
        }
    }

    // ━━━ 文本 ━━━

    pub fn put_text(&self, key: impl Into<String>, text: TextDrawable) {
        let mut inner = self.inner.write().unwrap();
        inner.texts.insert(key.into(), vec![text]);
        Self::mark_dirty(&mut inner);
    }

    pub fn put_text_list(&self, key: impl Into<String>, list: Vec<TextDrawable>) {
        let mut inner = self.inner.write().unwrap();
        if list.is_empty() {
            inner.texts.remove(&key.into());
        } else {
            inner.texts.insert(key.into(), list);
        }
        Self::mark_dirty(&mut inner);
    }

    pub fn remove_text(&self, key: &str) {
        let mut inner = self.inner.write().unwrap();
        if inner.texts.remove(key).is_some() {
            Self::mark_dirty(&mut inner);
        }
    }

    // ━━━ 线段 ━━━

    pub fn put_line(&self, key: impl Into<String>, line: LineDrawable) {
        let mut inner = self.inner.write().unwrap();
        inner.lines.insert(key.into(), vec![line]);
        Self::mark_dirty(&mut inner);
    }

    pub fn put_line_list(&self, key: impl Into<String>, list: Vec<LineDrawable>) {
        let mut inner = self.inner.write().unwrap();
        if list.is_empty() {
            inner.lines.remove(&key.into());
        } else {
            inner.lines.insert(key.into(), list);
        }
        Self::mark_dirty(&mut inner);
    }

    pub fn remove_line(&self, key: &str) {
        let mut inner = self.inner.write().unwrap();
        if inner.lines.remove(key).is_some() {
            Self::mark_dirty(&mut inner);
        }
    }

    // ━━━ 匹配结果 ━━━

    pub fn put_match_result(&self, key: impl Into<String>, result: MatchResultDrawable) {
        let mut inner = self.inner.write().unwrap();
        inner.match_results.insert(key.into(), vec![result]);
        Self::mark_dirty(&mut inner);
    }

    pub fn put_match_result_list(&self, key: impl Into<String>, list: Vec<MatchResultDrawable>) {
        let mut inner = self.inner.write().unwrap();
        if list.is_empty() {
            inner.match_results.remove(&key.into());
        } else {
            inner.match_results.insert(key.into(), list);
        }
        Self::mark_dirty(&mut inner);
    }

    pub fn remove_match_result(&self, key: &str) {
        let mut inner = self.inner.write().unwrap();
        if inner.match_results.remove(key).is_some() {
            Self::mark_dirty(&mut inner);
        }
    }

    // ━━━ 十字准星 ━━━

    pub fn put_crosshair(&self, key: impl Into<String>, crosshair: CrosshairDrawable) {
        let mut inner = self.inner.write().unwrap();
        inner.crosshairs.insert(key.into(), vec![crosshair]);
        Self::mark_dirty(&mut inner);
    }

    pub fn remove_crosshair(&self, key: &str) {
        let mut inner = self.inner.write().unwrap();
        if inner.crosshairs.remove(key).is_some() {
            Self::mark_dirty(&mut inner);
        }
    }

    // ━━━ 进度条 ━━━

    pub fn put_progress_bar(&self, key: impl Into<String>, bar: ProgressBarDrawable) {
        let mut inner = self.inner.write().unwrap();
        inner.progress_bars.insert(key.into(), vec![bar]);
        Self::mark_dirty(&mut inner);
    }

    pub fn remove_progress_bar(&self, key: &str) {
        let mut inner = self.inner.write().unwrap();
        if inner.progress_bars.remove(key).is_some() {
            Self::mark_dirty(&mut inner);
        }
    }

    // ━━━ 通用操作 ━━━

    /// 清空所有绘制内容
    pub fn clear_all(&self) {
        let mut inner = self.inner.write().unwrap();
        let had_content = !inner.rects.is_empty()
            || !inner.texts.is_empty()
            || !inner.lines.is_empty()
            || !inner.match_results.is_empty()
            || !inner.crosshairs.is_empty()
            || !inner.progress_bars.is_empty();
        inner.rects.clear();
        inner.texts.clear();
        inner.lines.clear();
        inner.match_results.clear();
        inner.crosshairs.clear();
        inner.progress_bars.clear();
        if had_content {
            inner.dirty = true;
        }
    }

    /// 获取所有绘制内容的快照（用于渲染）
    pub fn snapshot(&self) -> DrawSnapshot {
        let inner = self.inner.read().unwrap();
        DrawSnapshot {
            rects: inner.rects.values().flatten().cloned().collect(),
            texts: inner.texts.values().flatten().cloned().collect(),
            lines: inner.lines.values().flatten().cloned().collect(),
            match_results: inner.match_results.values().flatten().cloned().collect(),
            crosshairs: inner.crosshairs.values().flatten().cloned().collect(),
            progress_bars: inner.progress_bars.values().flatten().cloned().collect(),
        }
    }

    /// 检查是否有内容
    pub fn is_empty(&self) -> bool {
        let inner = self.inner.read().unwrap();
        inner.rects.is_empty()
            && inner.texts.is_empty()
            && inner.lines.is_empty()
            && inner.match_results.is_empty()
            && inner.crosshairs.is_empty()
            && inner.progress_bars.is_empty()
    }
}

impl Default for DrawContent {
    fn default() -> Self {
        Self::new()
    }
}

/// 绘制内容快照（渲染时使用，避免长时间持锁）
#[derive(Debug, Clone)]
pub struct DrawSnapshot {
    pub rects: Vec<RectDrawable>,
    pub texts: Vec<TextDrawable>,
    pub lines: Vec<LineDrawable>,
    pub match_results: Vec<MatchResultDrawable>,
    pub crosshairs: Vec<CrosshairDrawable>,
    pub progress_bars: Vec<ProgressBarDrawable>,
}

impl DrawSnapshot {
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
            && self.texts.is_empty()
            && self.lines.is_empty()
            && self.match_results.is_empty()
            && self.crosshairs.is_empty()
            && self.progress_bars.is_empty()
    }
}
