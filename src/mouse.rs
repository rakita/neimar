use ratatui::layout::Rect;

/// Returns the visible session index if the click lands on a session row, or `None`.
pub fn clicked_session_index(
    column: u16,
    row: u16,
    sessions_area: Rect,
    visible_count: usize,
    item_height: usize,
    list_scroll_offset: usize,
) -> Option<usize> {
    let inner = sessions_area.inner(ratatui::layout::Margin::new(1, 1));
    if column < inner.x || column >= inner.x + inner.width {
        return None;
    }
    if row < inner.y || row >= inner.y + inner.height {
        return None;
    }
    let offset = (row - inner.y) as usize;
    let index = list_scroll_offset + offset / item_height;
    if index >= visible_count {
        return None;
    }
    Some(index)
}
