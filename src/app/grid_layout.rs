//! Matemática de la cuadrícula: columnas, navegación con teclado y rango de miniaturas a mantener.
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Nav {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

/// Columnas que caben en `width` con celdas de `cell` y separación `gap` (mínimo 1).
pub(crate) fn columns(width: f32, cell: f32, gap: f32) -> usize {
    if cell <= 0.0 {
        return 1;
    }
    (((width + gap) / (cell + gap)).floor() as usize).max(1)
}

pub(crate) fn row_count(total: usize, cols: usize) -> usize {
    total.div_ceil(cols.max(1))
}

/// Nuevo índice seleccionado; `page_rows` filas por página. Se mantiene dentro de `0..total`.
pub(crate) fn navigate(
    index: usize,
    nav: Nav,
    cols: usize,
    page_rows: usize,
    total: usize,
) -> usize {
    if total == 0 {
        return 0;
    }
    let cols = cols.max(1);
    let last = total - 1;
    let index = index.min(last);
    let page = page_rows.max(1) * cols;
    match nav {
        Nav::Left => index.saturating_sub(1),
        Nav::Right => (index + 1).min(last),
        Nav::Up => index.checked_sub(cols).unwrap_or(index),
        Nav::Down if index + cols <= last => index + cols,
        // Desde la penúltima fila, bajar lleva a la última imagen de la fila incompleta.
        Nav::Down if index / cols < last / cols => last,
        Nav::Down => index,
        Nav::PageUp => index.saturating_sub(page),
        Nav::PageDown => (index + page).min(last),
        Nav::Home => 0,
        Nav::End => last,
    }
}

/// Índices de las filas visibles más `margin_rows` arriba y abajo.
pub(crate) fn keep_range(
    rows: Range<usize>,
    margin_rows: usize,
    cols: usize,
    total: usize,
) -> Range<usize> {
    let start = (rows.start.saturating_sub(margin_rows) * cols).min(total);
    let end = ((rows.end + margin_rows) * cols).min(total);
    start..end.max(start)
}

/// Offset vertical para que la fila `row` quede visible, o `None` si ya lo está.
pub(crate) fn scroll_to_row(row: usize, visible: Range<usize>, row_pitch: f32) -> Option<f32> {
    if row < visible.start {
        Some(row as f32 * row_pitch)
    } else if row >= visible.end {
        let shown = visible.len().max(1);
        Some((row + 1 - shown) as f32 * row_pitch)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_fit_width_with_gaps_and_never_zero() {
        assert_eq!(columns(1000.0, 180.0, 8.0), 5);
        assert_eq!(columns(180.0, 180.0, 8.0), 1);
        assert_eq!(columns(50.0, 180.0, 8.0), 1);
        assert_eq!(columns(1000.0, 0.0, 0.0), 1);
    }

    #[test]
    fn row_count_rounds_up() {
        assert_eq!(row_count(0, 5), 0);
        assert_eq!(row_count(10, 5), 2);
        assert_eq!(row_count(11, 5), 3);
    }

    #[test]
    fn navigate_moves_and_clamps() {
        let (cols, page, total) = (5, 3, 23);
        assert_eq!(navigate(7, Nav::Left, cols, page, total), 6);
        assert_eq!(navigate(0, Nav::Left, cols, page, total), 0);
        assert_eq!(navigate(7, Nav::Right, cols, page, total), 8);
        assert_eq!(navigate(22, Nav::Right, cols, page, total), 22);
        assert_eq!(navigate(7, Nav::Up, cols, page, total), 2);
        assert_eq!(navigate(2, Nav::Up, cols, page, total), 2);
        assert_eq!(navigate(7, Nav::Down, cols, page, total), 12);
        assert_eq!(
            navigate(19, Nav::Down, cols, page, total),
            22,
            "última fila incompleta"
        );
        assert_eq!(navigate(22, Nav::Down, cols, page, total), 22);
        assert_eq!(navigate(20, Nav::PageUp, cols, page, total), 5);
        assert_eq!(navigate(2, Nav::PageDown, cols, page, total), 17);
        assert_eq!(navigate(12, Nav::Home, cols, page, total), 0);
        assert_eq!(navigate(3, Nav::End, cols, page, total), 22);
        assert_eq!(navigate(0, Nav::End, cols, page, 0), 0);
    }

    #[test]
    fn keep_range_adds_margin_and_clamps() {
        assert_eq!(keep_range(2..4, 1, 5, 100), 5..25);
        assert_eq!(keep_range(0..2, 3, 5, 100), 0..25);
        assert_eq!(keep_range(18..20, 2, 5, 97), 80..97);
        assert_eq!(keep_range(0..0, 2, 5, 0), 0..0);
    }

    #[test]
    fn scroll_to_row_only_when_outside_visible_rows() {
        assert_eq!(scroll_to_row(5, 3..8, 100.0), None);
        assert_eq!(scroll_to_row(1, 3..8, 100.0), Some(100.0));
        assert_eq!(
            scroll_to_row(9, 3..8, 100.0),
            Some(500.0),
            "queda como última visible"
        );
    }
}
