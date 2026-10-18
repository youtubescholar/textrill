// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! The window geometry the UI remembers.
//!
//! The one rule that keeps the window recoverable is that only the *normal*
//! size and the zoom are ever persisted -- never a position, never the
//! maximized size.
//!
//! * A stored position can point at a monitor that is no longer there, or at
//!   coordinates a different DPI no longer matches; on Wayland a client cannot
//!   set a position at all. The compositor already places the window somewhere
//!   visible, so storing a position only adds a way to open off-screen.
//! * Restoring a screen-sized *normal* size leaves a window flush with the work
//!   area, with no resize grips -- the "cannot resize" dead end. So while the
//!   window is maximized the last normal size is kept and only the maximized
//!   flag is stored.
//!
//! Everything read back is clamped, so a hand-edited or stale file is advisory:
//! the window can always be opened, and the `View → Reset window size and zoom`
//! command is the single action that returns to the default.

/// Smallest inner size, in egui points. Below this the controls clip.
pub const MIN_SIZE: [f32; 2] = [360.0, 300.0];

/// The size a first run opens at, and what the reset command restores.
pub const DEFAULT_SIZE: [f32; 2] = [900.0, 720.0];

/// A ceiling on a stored size, so a corrupt file cannot ask for a window no
/// screen has. `eframe` clamps the value to the actual monitor as well.
pub const MAX_SIZE: [f32; 2] = [16384.0, 16384.0];

/// The zoom range the UI accepts. Four times leaves room for the 200% that
/// WCAG 1.4.4 asks for, while staying a value a user can undo.
pub const MIN_ZOOM: f32 = 0.5;
pub const MAX_ZOOM: f32 = 4.0;

/// Parse a stored `WIDTHxHEIGHT` size, clamped to a usable range.
///
/// Text that cannot be read as two numbers yields `None`, so the caller falls
/// back to [`DEFAULT_SIZE`]. A readable but out-of-range value is clamped, so
/// "10x10" opens small rather than microscopic and "100000x100000" opens large
/// rather than off the screen.
pub fn parse_size(value: &str) -> Option<[f32; 2]> {
    let (width, height) = value.trim().split_once(['x', 'X'])?;
    let width: f32 = width.trim().parse().ok()?;
    let height: f32 = height.trim().parse().ok()?;
    Some(clamp_size([width, height]))
}

/// Format a size for storage. Rounded to whole points: sub-point precision is
/// not meaningful across a restart and makes the file harder to read by hand.
pub fn format_size([width, height]: [f32; 2]) -> String {
    format!("{}x{}", width.round() as i64, height.round() as i64)
}

/// Clamp each dimension into `[MIN_SIZE, MAX_SIZE]`. A non-finite or
/// non-positive value falls back to the default for that axis.
pub fn clamp_size([width, height]: [f32; 2]) -> [f32; 2] {
    [clamp_axis(width, 0), clamp_axis(height, 1)]
}

fn clamp_axis(value: f32, axis: usize) -> f32 {
    if !value.is_finite() || value <= 0.0 {
        return DEFAULT_SIZE[axis];
    }
    value.clamp(MIN_SIZE[axis], MAX_SIZE[axis])
}

/// Parse a stored zoom factor and clamp it. Unreadable text yields `None`.
pub fn parse_zoom(value: &str) -> Option<f32> {
    let zoom: f32 = value.trim().parse().ok()?;
    Some(clamp_zoom(zoom))
}

/// Clamp a zoom factor into `[MIN_ZOOM, MAX_ZOOM]`; a nonsensical value means
/// 100%.
pub fn clamp_zoom(zoom: f32) -> f32 {
    if !zoom.is_finite() || zoom <= 0.0 {
        return 1.0;
    }
    zoom.clamp(MIN_ZOOM, MAX_ZOOM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_size_reads_back() {
        assert_eq!(parse_size("900x720"), Some([900.0, 720.0]));
        assert_eq!(parse_size(" 1024 x 768 "), Some([1024.0, 768.0]));
        assert_eq!(parse_size("800X600"), Some([800.0, 600.0]));
    }

    #[test]
    fn a_size_is_clamped_on_the_way_in() {
        assert_eq!(parse_size("10x10"), Some(MIN_SIZE));
        assert_eq!(parse_size("100000x100000"), Some(MAX_SIZE));
        assert_eq!(parse_size("0x720"), Some([DEFAULT_SIZE[0], 720.0]));
    }

    #[test]
    fn nonsense_sizes_are_rejected_not_guessed() {
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("wide"), None);
        assert_eq!(parse_size("900"), None);
        assert_eq!(parse_size("900x"), None);
    }

    #[test]
    fn a_size_round_trips() {
        assert_eq!(format_size([900.0, 720.0]), "900x720");
        assert_eq!(
            parse_size(&format_size([640.4, 480.6])),
            Some([640.0, 481.0])
        );
    }

    #[test]
    fn zoom_is_clamped_and_nonsense_is_100_percent() {
        assert_eq!(parse_zoom("1.5"), Some(1.5));
        assert_eq!(parse_zoom("0.01"), Some(MIN_ZOOM));
        assert_eq!(parse_zoom("99"), Some(MAX_ZOOM));
        assert_eq!(parse_zoom("wide"), None);
        assert_eq!(clamp_zoom(f32::NAN), 1.0);
        assert_eq!(clamp_zoom(-2.0), 1.0);
    }
}
