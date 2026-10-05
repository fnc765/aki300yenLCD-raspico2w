//! LCD 出力の 180 度回転。描画と同期の順序を変えず、転送時に画素の並びを反転する。

/// 出力行に対応する入力行。
pub fn source_row(y: usize, height: usize, rotate_180: bool) -> usize {
    if rotate_180 { height - 1 - y } else { y }
}

/// 入力画素を変換しながら出力へ写す。回転時は行内の左右も反転する。
pub fn copy_pixels<T: Copy, U>(src: &[T], dst: &mut [U], rotate_180: bool, mut convert: impl FnMut(T) -> U) {
    assert_eq!(src.len(), dst.len());
    if rotate_180 {
        for (d, &s) in dst.iter_mut().zip(src.iter().rev()) {
            *d = convert(s);
        }
    } else {
        for (d, &s) in dst.iter_mut().zip(src) {
            *d = convert(s);
        }
    }
}
