// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! The window's own themes, font sizes and motion, rather than egui's stock
//! look.
//!
//! `egui` has two built-in themes, light and dark, and lets a program replace
//! the [`egui::Style`] used for both. That is the seam used here: every theme
//! in this module is a full [`egui::Style`] the window owns, so there is no
//! third-party "skin" dependency and the colors are the window's to control.
//!
//! The palettes are the ones `textrill/templates/htmltemplate-basicui.html`
//! already verified against the WCAG contrast rules, transferred onto the
//! surfaces a desktop window needs. The [`contrast_ratio`] tests in this
//! module pin them: every colour that carries text is at least 7:1 (WCAG AAA
//! for body text) against the surface it is drawn on, so a theme change can
//! never silently land on a low-contrast pair.
//!
//! Besides the palette the style carries three knobs the window exposes:
//!
//! - **Text size** — a multiplier over the built-in type scale, in the spirit
//!   of a browser's plain-text size (as opposed to the whole-UI zoom, which
//!   scales everything including buttons). The size that governs an
//!   interaction target is scaled with it, so a bigger type also brings a
//!   bigger hit area.
//! - **Target size** — the minimum clickable height, `egui::Spacing`'s
//!   `interact_size`. WCAG 2.2 makes 24×24 CSS px the AA minimum target size
//!   and 44×44 the AAA one; the ordinary themes use 24, and High contrast uses
//!   44 on the reasoning that a theme who wants maximum clarity wants maximum
//!   targets with it.
//! - **Reduce motion** — `animation_time` set to zero, so nothing slides or
//!   fades; egui's own menu fade is the only motion the window has, but an
//!   environment that asks for reduced motion gets the guarantee rather than
//!   the default 0.2s animation.

use egui::{Color32, TextStyle, Theme as EguiTheme};

/// The 100% text size. The Display row's "Text" control steps around this.
pub const DEFAULT_TEXT_SIZE: f32 = 1.0;

/// The WCAG 2.2 AA minimum target size, as `interact_size.y` in points (CSS px
/// at scale 1.0).
const TARGET_SIZE_AA: f32 = 24.0;
/// The WCAG 2.2 AAA minimum target size, used by High contrast.
const TARGET_SIZE_AAA: f32 = 44.0;

/// A step the "Text" size control offers. `t` is the multiplier over the
/// built-in type scale; Firefox's largest convenience size is 200%, which is
/// why it stops there.
pub const TEXT_SIZES: &[(f32, &str)] = &[
    (0.8, "Smaller"),
    (0.9, "Small"),
    (1.0, "Default"),
    (1.25, "Large"),
    (1.5, "Larger"),
    (2.0, "Maximum"),
];

/// A text-size multiplier, stepped from [`TEXT_SIZES`].
///
/// A non-finite value (a hand-edited `text-size=nan`) falls back to the
/// default rather than being clamped to `NaN`, which would propagate into the
/// type scale and panic egui's layout; this is the same guard
/// [`crate::window_state::clamp_zoom`] uses.
pub fn clamp_text_size(size: f32) -> f32 {
    if !size.is_finite() {
        return DEFAULT_TEXT_SIZE;
    }
    size.clamp(TEXT_SIZES[0].0, TEXT_SIZES[TEXT_SIZES.len() - 1].0)
}

/// The theme the window shows.
///
/// The variant's own name is what is persisted (see [`Self::from_stored`]), so
/// renaming a variant breaks nobody's configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// The paper look the basicui template's default reader uses: warm off-
    /// white, near-black ink, blue accent.
    #[default]
    Light,
    /// The template's dark chrome: near-black panel, light text, blue accent.
    Dark,
    /// The reader's sepia: the low-glare paper many long-session readers pick.
    Sepia,
    /// Maximum clarity: pure white on black, cyan links, gold focus.
    HighContrast,
}

impl Theme {
    /// Every theme, in the order the picker lists them.
    pub const ALL: [Theme; 4] = [Theme::Light, Theme::Dark, Theme::Sepia, Theme::HighContrast];

    /// The name shown in the picker (and stored under the `theme` key).
    pub fn name(self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::Sepia => "Sepia",
            Theme::HighContrast => "High contrast",
        }
    }

    /// The stored spelling of the theme name, which is the variant name.
    pub fn stored(self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::Sepia => "Sepia",
            Theme::HighContrast => "HighContrast",
        }
    }

    /// Parse a stored `theme` value, case-insensitively; `None` for garbage.
    pub fn from_stored(value: &str) -> Option<Theme> {
        Theme::ALL
            .iter()
            .copied()
            .find(|theme| theme.stored().eq_ignore_ascii_case(value.trim()))
    }

    /// The palette this theme paints with. See [`Palette`].
    pub fn palette(self) -> &'static Palette {
        match self {
            Theme::Light => &LIGHT,
            Theme::Dark => &DARK,
            Theme::Sepia => &SEPIA,
            Theme::HighContrast => &HIGH_CONTRAST,
        }
    }

    /// Whether egui should consider the base theme dark; kept in step so its
    /// own defaults for anything the palette does not name agree with the look.
    fn egui_theme(self) -> EguiTheme {
        if self.palette().dark_mode {
            EguiTheme::Dark
        } else {
            EguiTheme::Light
        }
    }

    /// The minimum clickable height for this theme's widgets.
    fn target_size(self) -> f32 {
        match self {
            Theme::HighContrast => TARGET_SIZE_AAA,
            _ => TARGET_SIZE_AA,
        }
    }
}

/// A fixed colour scheme. Members are chosen against lighter/darker "chrome"
/// surfaces so the fourteen contrast tests at the bottom stay honest.
///
/// `surface` is the window background, `surface_alt` the background behind
/// controls and editors. `text` is the ink on either. `muted` is secondary
/// text still above AAA. `accent`/`accent_text` are the pressed/focused pair,
/// `hover` the hovered control background, `hyperlink` link ink.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark_mode: bool,
    pub surface: Color32,
    pub surface_alt: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub hover: Color32,
    pub hyperlink: Color32,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// The `F5F5DC  / 1A1A1A` bar and `004B87/white` accent the basicui template
/// verified for its top chrome, laid over white panels.
static LIGHT: Palette = Palette {
    dark_mode: false,
    surface: rgb(0xF5, 0xF5, 0xDC),
    surface_alt: rgb(0xFF, 0xFF, 0xFF),
    text: rgb(0x1A, 0x1A, 0x1A),
    muted: rgb(0x4D, 0x4D, 0x43),
    border: rgb(0xC9, 0xC9, 0xB6),
    accent: rgb(0x00, 0x4B, 0x87),
    accent_text: rgb(0xFF, 0xFF, 0xFF),
    hover: rgb(0xEC, 0xE8, 0xD2),
    hyperlink: rgb(0x00, 0x4B, 0x87),
};

/// The template's `1E1E1E / E0E0E0` panel with `2C2C2C` controls, the blue
/// `004B87` accent and white selected text.
static DARK: Palette = Palette {
    dark_mode: true,
    surface: rgb(0x1E, 0x1E, 0x1E),
    surface_alt: rgb(0x2C, 0x2C, 0x2C),
    text: rgb(0xE0, 0xE0, 0xE0),
    muted: rgb(0xAA, 0xAA, 0xAA),
    border: rgb(0x3A, 0x3A, 0x3A),
    accent: rgb(0x00, 0x4B, 0x87),
    accent_text: rgb(0xFF, 0xFF, 0xFF),
    hover: rgb(0x38, 0x38, 0x38),
    hyperlink: rgb(0xA0, 0xD2, 0xFF),
};

/// The reader's `F4ECD8 / 5B4636` sepia with the same blue link the template
/// gave its sepia content. The accent is a deeper brown than the template's,
/// because `8B4513` sits just at the AAA edge of white ink instead of above
/// it; `7A3B0F` clears it comfortably. The hover is `F2EACF`, a shade the
/// tests verified (the first candidate was too dark, dropping the ink on it
/// below AAA).
static SEPIA: Palette = Palette {
    dark_mode: false,
    surface: rgb(0xF4, 0xEC, 0xD8),
    surface_alt: rgb(0xFB, 0xF5, 0xE2),
    text: rgb(0x5B, 0x46, 0x36),
    muted: rgb(0x4E, 0x3B, 0x2C),
    border: rgb(0xC9, 0xB2, 0x8F),
    accent: rgb(0x7A, 0x3B, 0x0F),
    accent_text: rgb(0xFF, 0xFF, 0xFF),
    hover: rgb(0xF2, 0xEA, 0xCF),
    hyperlink: rgb(0x00, 0x44, 0x99),
};

/// The template's high-contrast theme: `000000`, white ink, gold focus.
static HIGH_CONTRAST: Palette = Palette {
    dark_mode: true,
    surface: rgb(0x00, 0x00, 0x00),
    surface_alt: rgb(0x0A, 0x0A, 0x0A),
    text: rgb(0xFF, 0xFF, 0xFF),
    muted: rgb(0xCC, 0xCC, 0xCC),
    border: rgb(0xE0, 0xE0, 0xE0),
    accent: rgb(0xFF, 0xD7, 0x00),
    accent_text: rgb(0x00, 0x00, 0x00),
    hover: rgb(0x1C, 0x1C, 0x1C),
    hyperlink: rgb(0x00, 0xFF, 0xFF),
};

/// The WCAG relative luminance of a colour, from the sRGB channel values.
fn luminance(c: Color32) -> f32 {
    let linear = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
}

/// The WCAG contrast ratio between two colours, `fg` rendered on `bg`, as
/// `(L1 + 0.05) / (L2 + 0.05)`. 7.0 is the AAA requirement for normal text.
pub fn contrast_ratio(fg: Color32, bg: Color32) -> f32 {
    let (hi, lo) = {
        let (a, b) = (luminance(fg), luminance(bg));
        if a > b {
            (a, b)
        } else {
            (b, a)
        }
    };
    (hi + 0.05) / (lo + 0.05)
}

/// The visual state of a control: text ink, fill and outline.
fn widget(text: Color32, fill: Color32, stroke: Color32) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: egui::Stroke::new(1.0, stroke),
        corner_radius: egui::CornerRadius::same(4),
        expansion: 0.0,
        fg_stroke: egui::Stroke::new(1.0, text),
    }
}

/// The full [`egui::Style`] the window wants for `theme` at `text_size` with
/// `reduce_motion`. Applying it to *both* of egui's themes keeps popups, menus
/// and the side panels in step whichever internal theme is active.
pub fn build_style(theme: Theme, text_size: f32, reduce_motion: bool) -> egui::Style {
    let palette = theme.palette();
    let text_size = clamp_text_size(text_size);
    let mut style = egui::Style::default();

    // Type scale: every built-in size times the multiplier. egui sizes its
    // widgets from these, so a font change reflows the whole window.
    for font_id in style.text_styles.values_mut() {
        font_id.size *= text_size;
    }

    // Minimum clickable height. `interact_size.y` is the default height of a
    // button; the horizontal extent is already covered by the labels a control
    // needs. The more generous 24 px clears the WCAG 2.2 AA minimum for the
    // ordinary themes, 44 px (AAA) is what High contrast asks for on top.
    style.spacing.interact_size.y = theme.target_size() * text_size;

    // The only motion egui has is the 0.2s panel/popup fade. Reduced motion
    // switches it off rather than shortening it.
    style.animation_time = if reduce_motion { 0.0 } else { 0.2 };

    let mut visuals = if palette.dark_mode {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.dark_mode = palette.dark_mode;
    visuals.override_text_color = Some(palette.text);
    visuals.weak_text_color = Some(palette.muted);
    visuals.hyperlink_color = palette.hyperlink;

    // The high-contrast theme shows its borders so a control's bounds are
    // never a guess and its pressed/selected state is unmistakable.
    visuals.widgets.noninteractive = widget(palette.text, palette.surface_alt, palette.border);
    visuals.widgets.inactive = widget(palette.text, palette.surface_alt, palette.border);
    visuals.widgets.hovered = widget(palette.text, palette.hover, palette.accent);
    // A focused widget is styled `active`: its thick outline is the focus
    // ring. Making it the accent gives every theme a visible, theme-aware
    // focus indicator (blue on the light themes, gold on High contrast).
    visuals.widgets.active = widget(palette.accent_text, palette.accent, palette.accent);
    visuals.widgets.open = widget(palette.accent_text, palette.accent, palette.accent);

    visuals.selection = egui::style::Selection {
        bg_fill: palette.accent,
        stroke: egui::Stroke::new(1.0, palette.accent_text),
    };

    visuals.panel_fill = palette.surface;
    visuals.window_fill = palette.surface;
    visuals.window_stroke = egui::Stroke::new(1.0, palette.border);
    visuals.faint_bg_color = palette.surface_alt;
    visuals.extreme_bg_color = palette.surface_alt;
    visuals.text_edit_bg_color = Some(palette.surface_alt);
    visuals.code_bg_color = palette.surface_alt;

    style.visuals = visuals;
    style
}

/// Put `theme`'s style in effect on `ctx` (both of egui's internal themes, see
/// [`build_style`]) and record the theme preference so `ctx.theme()` matches
/// what is on screen.
pub fn apply(ctx: &egui::Context, theme: Theme, text_size: f32, reduce_motion: bool) {
    let style = build_style(theme, text_size, reduce_motion);
    ctx.all_styles_mut(move |current| *current = style.clone());
    ctx.set_theme(theme.egui_theme());
}

/// The face for running text, for tests and the Display row's readout.
pub fn body_font_size(style: &egui::Style) -> f32 {
    let font = style
        .text_styles
        .get(&TextStyle::Body)
        .expect("egui always has a Body text style");
    font.size
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_pair(a: Color32, b: Color32, what: &str) {
        let ratio = contrast_ratio(a, b);
        assert!(
            ratio >= 7.0,
            "{what}: {ratio:.2}:1 is below the AAA 7:1 floor (0x{:02X}{:02X}{:02X} on 0x{:02X}{:02X}{:02X})",
            a.r(),
            a.g(),
            a.b(),
            b.r(),
            b.g(),
            b.b(),
        );
    }

    /// Every theme carries every text-bearing colour at AAA against the
    /// surface it is drawn on, so a theme switch can never hand the user a
    /// low-contrast pair. This is the contract the whole of `theme.rs` exists
    /// to keep.
    #[test]
    fn every_palette_is_aaa_on_its_surfaces() {
        for theme in Theme::ALL {
            let p = theme.palette();
            assert_pair(p.text, p.surface, &format!("{} text/surface", theme.name()));
            assert_pair(
                p.text,
                p.surface_alt,
                &format!("{} text/surface_alt", theme.name()),
            );
            assert_pair(
                p.muted,
                p.surface,
                &format!("{} muted/surface", theme.name()),
            );
            assert_pair(
                p.accent_text,
                p.accent,
                &format!("{} accent_text/accent", theme.name()),
            );
            assert_pair(p.text, p.hover, &format!("{} text/hover", theme.name()));
            assert_pair(
                p.hyperlink,
                p.surface,
                &format!("{} hyperlink/surface", theme.name()),
            );
        }
    }

    /// The named ratio the basicui template verified its own chrome at, so
    /// this palette really is that template's, not just "some dark colours".
    #[test]
    fn light_worded_fully() {
        assert_pair(rgb(0x1A, 0x1A, 0x1A), rgb(0xF5, 0xF5, 0xDC), "light worded");
    }

    /// High contrast is exact: the theme is a guarantee of maximum contrast,
    /// so its values are the ceiling of contrast, not merely above a floor.
    #[test]
    fn high_contrast_ink_is_maximum() {
        let p = Theme::HighContrast.palette();
        assert_eq!(p.surface, Color32::BLACK);
        assert_eq!(p.text, Color32::WHITE);
        assert!(contrast_ratio(p.text, p.surface) > 20.0);
    }

    /// Muted text is a secondary weight, never a copy of the ink: on the dark
    /// themes it is distinctly dimmer, and it is always a different visual from
    /// the background. Its accessibility (`≥ 7:1` on the surface it sits on) is
    /// already covered by [`every_palette_is_aaa_on_its_surfaces`].
    #[test]
    fn muted_is_a_secondary_weight_distinct_from_the_ink() {
        for theme in Theme::ALL {
            let p = theme.palette();
            assert_ne!(
                p.muted,
                p.text,
                "{}: muted must differ from the ink",
                theme.name()
            );
            assert_ne!(
                p.muted,
                p.surface,
                "{}: muted must differ from the background",
                theme.name()
            );
            if p.dark_mode {
                assert!(
                    luminance(p.muted) < luminance(p.text),
                    "{}: on a dark surface muted should be dimmer than the ink",
                    theme.name()
                );
            }
        }
    }

    /// A non-finite stored size is the default, never `NaN`: a `text-size=nan`
    /// line in the config must not reach the type scale and panic the layout.
    #[test]
    fn nonsense_text_size_falls_back_to_the_default() {
        assert_eq!(clamp_text_size(f32::NAN), DEFAULT_TEXT_SIZE);
        assert_eq!(clamp_text_size(f32::INFINITY), DEFAULT_TEXT_SIZE);
        assert_eq!(clamp_text_size(f32::NEG_INFINITY), DEFAULT_TEXT_SIZE);
        assert_eq!(clamp_text_size(5.0), TEXT_SIZES[TEXT_SIZES.len() - 1].0);
        assert_eq!(clamp_text_size(0.1), TEXT_SIZES[0].0);
        assert!(build_style(Theme::Light, f32::NAN, false)
            .spacing
            .interact_size
            .y
            .is_finite());
    }

    /// The multiplier scales every entry of the type scale together.
    #[test]
    fn text_size_scales_the_type_scale() {
        for theme in Theme::ALL {
            let base = build_style(theme, 1.0, false);
            let doubled = build_style(theme, 2.0, false);
            for (key, base_font) in &base.text_styles {
                let big = doubled.text_styles[key].size;
                assert!(
                    (big - base_font.size * 2.0).abs() < 1e-3,
                    "{} {key:?}: {} -> {}",
                    theme.name(),
                    base_font.size,
                    big
                );
            }
        }
    }

    /// Reduced motion kills the animation outright rather than shortening it.
    #[test]
    fn reduce_motion_zeroes_animation_time() {
        for theme in Theme::ALL {
            assert!(build_style(theme, 1.0, false).animation_time > 0.0);
            assert_eq!(build_style(theme, 1.0, true).animation_time, 0.0);
        }
    }

    /// The clickable height clears the WCAG 2.2 minimum target, and High
    /// contrast asks for the AAA target on top.
    #[test]
    fn every_theme_has_a_generous_target_size() {
        for theme in Theme::ALL {
            let target = build_style(theme, 1.0, false).spacing.interact_size.y;
            let floor = theme.target_size();
            assert!(
                target >= floor - 1e-3,
                "{}: {target} is below {floor}",
                theme.name()
            );
        }
    }

    #[test]
    fn stored_names_round_trip() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_stored(theme.stored()), Some(theme));
            assert_eq!(
                Theme::from_stored(&theme.stored().to_lowercase()),
                Some(theme)
            );
        }
        assert_eq!(Theme::from_stored("neon"), None);
    }
}
