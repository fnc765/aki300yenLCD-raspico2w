//! Compact scanout geometry. Each active row has two repeated border words and
//! 400 visible words; blanking reuses one SRAM word. DMA expands these segments.

pub const WIDTH: usize = 400;
pub const HEIGHT: usize = 96;
pub const LINE_WORDS: usize = 509;
pub const TOP_LINES: usize = 16;
pub const FRAME_LINES: usize = 113;
pub const LEFT_WORDS: usize = 106;
pub const RIGHT_WORDS: usize = LINE_WORDS - LEFT_WORDS - WIDTH;
pub const BOTTOM_INDEX: usize = 1 + HEIGHT * 3;
pub const RELOAD_INDEX: usize = BOTTOM_INDEX + 1;
pub const SEGMENT_COUNT: usize = RELOAD_INDEX + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Black,
    Left(usize),
    Pixels(usize),
    Right(usize),
    Reload,
}

pub fn segment(index: usize) -> (Source, usize) {
    match index {
        0 => (Source::Black, TOP_LINES * LINE_WORDS),
        BOTTOM_INDEX => (Source::Black, (FRAME_LINES - TOP_LINES - HEIGHT) * LINE_WORDS),
        RELOAD_INDEX => (Source::Reload, 1),
        _ => {
            let row = (index - 1) / 3;
            match (index - 1) % 3 {
                0 => (Source::Left(row), LEFT_WORDS),
                1 => (Source::Pixels(row), WIDTH),
                _ => (Source::Right(row), RIGHT_WORDS),
            }
        }
    }
}

/// CH2 points just beyond the descriptor currently running on CH0. Only the
/// early blanking window is used for present(); transitional states are late.
pub fn current_line(next_segment: usize, remaining: usize) -> usize {
    match next_segment {
        1 => (TOP_LINES * LINE_WORDS).saturating_sub(remaining) / LINE_WORDS,
        2..=BOTTOM_INDEX => TOP_LINES + (next_segment - 2) / 3,
        _ => FRAME_LINES - 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_scan_matches_full_frame_every_word() {
        let mut position = 0;
        for index in 0..RELOAD_INDEX {
            let (source, count) = segment(index);
            for x in 0..count {
                let row = position / LINE_WORDS;
                let column = position % LINE_WORDS;
                let expected = if (TOP_LINES..TOP_LINES + HEIGHT).contains(&row) {
                    let y = row - TOP_LINES;
                    (y * WIDTH + column.saturating_sub(LEFT_WORDS).min(WIDTH - 1) + 1) as u32
                } else {
                    0
                };
                let actual = match source {
                    Source::Black => 0,
                    Source::Left(y) => (y * WIDTH + 1) as u32,
                    Source::Pixels(y) => (y * WIDTH + x + 1) as u32,
                    Source::Right(y) => ((y + 1) * WIDTH) as u32,
                    Source::Reload => unreachable!(),
                };
                assert_eq!(actual, expected, "scan word {position}");
                position += 1;
            }
        }
        assert_eq!(position, LINE_WORDS * FRAME_LINES);
        assert_eq!(segment(RELOAD_INDEX), (Source::Reload, 1));
    }

    #[test]
    fn present_window_excludes_active_and_reload_segments() {
        assert_eq!(current_line(1, TOP_LINES * LINE_WORDS), 0);
        assert_eq!(current_line(1, 4 * LINE_WORDS), 12);
        for next in 2..=SEGMENT_COUNT {
            assert!(current_line(next, 0) >= TOP_LINES);
        }
        assert!(current_line(0, 0) >= TOP_LINES);
    }
}
