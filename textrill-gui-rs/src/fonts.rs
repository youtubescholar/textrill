// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Bundled fonts.
//!
//! `egui` renders only the fonts it is given; it never consults the operating
//! system's font configuration. The stock set covers Latin, so this adds two
//! fallbacks without a system-font dependency that a Flatpak sandbox could not
//! honour:
//!
//! - **Noto Sans** — widens Greek and Cyrillic.
//! - **Noto Sans CJK TC** — the full pan-CJK repertoire (CJK Unified
//!   Ideographs, Extension A, kana, Hangul, Bopomofo). `egui` applies no
//!   OpenType language features, so the `TC` file is chosen over `SC` for its
//!   Traditional-default glyph shapes; coverage is identical either way, and
//!   Simplified codepoints are distinct characters that render normally.
//!
//! Both are licensed under the SIL Open Font License 1.1; the licence texts
//! travel with the source in `assets/fonts/`.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// Noto Sans Regular, hinted (`notofonts/noto-fonts`), OFL-1.1.
const NOTO_SANS_REGULAR: &[u8] = include_bytes!("../assets/fonts/NotoSans-Regular.ttf");
/// Noto Sans Bold, hinted (`notofonts/noto-fonts`), OFL-1.1 — the same family
/// as the Regular face above, so the existing `OFL.txt` covers it. It gives
/// the rendered preview (and only the rendered preview) a real bold face for
/// `<strong>` and headings; egui has no synthetic weight.
const NOTO_SANS_BOLD_TTF: &[u8] = include_bytes!("../assets/fonts/NotoSans-Bold.ttf");
/// Noto Sans CJK TC Regular, CFF outlines (`notofonts/noto-cjk`), OFL-1.1.
const NOTO_SANS_CJK_TC: &[u8] = include_bytes!("../assets/fonts/NotoSansCJKtc-Regular.otf");

/// The family names the bundled fonts are registered under.
const NOTO_SANS: &str = "noto_sans";
const NOTO_SANS_BOLD: &str = "noto_sans_bold";
const NOTO_SANS_CJK: &str = "noto_sans_cjk_tc";

/// The family the preview uses for `<strong>` and `<h1>`–`<h6>`: Noto Sans
/// Bold, falling back to the Regular face and then to the CJK face so bold
/// runs over Greek, Cyrillic or CJK never drop to tofu.
pub const BOLD_FAMILY: &str = NOTO_SANS_BOLD;

/// Add the bundled fonts to `ctx` as fallbacks behind the stock faces.
///
/// The stock families stay first, so Latin text is unchanged; the Noto faces
/// are only consulted for a glyph the stocks and the earlier fallbacks lack.
/// Both are appended to the proportional and monospace chains so the editor and
/// the preview alike can show the covered scripts.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        NOTO_SANS.to_owned(),
        Arc::new(FontData::from_static(NOTO_SANS_REGULAR)),
    );
    fonts.font_data.insert(
        NOTO_SANS_CJK.to_owned(),
        Arc::new(FontData::from_static(NOTO_SANS_CJK_TC)),
    );
    fonts.font_data.insert(
        NOTO_SANS_BOLD.to_owned(),
        Arc::new(FontData::from_static(NOTO_SANS_BOLD_TTF)),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        let chain = fonts.families.entry(family).or_default();
        chain.push(NOTO_SANS.to_owned());
        chain.push(NOTO_SANS_CJK.to_owned());
    }
    // A named family the preview selects per text run: bold first, so Latin
    // gets the weight, and the regular faces behind it as the fallback chain
    // so a bold run containing CJK falls through rather than disappearing.
    let bold = fonts
        .families
        .entry(FontFamily::Name(NOTO_SANS_BOLD.into()))
        .or_default();
    bold.push(NOTO_SANS_BOLD.to_owned());
    bold.push(NOTO_SANS.to_owned());
    bold.push(NOTO_SANS_CJK.to_owned());
    ctx.set_fonts(fonts);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_fonts_have_the_expected_sfnt_magic() {
        assert!(
            NOTO_SANS_REGULAR.len() > 100_000,
            "Noto Sans was not embedded"
        );
        // sfnt version 1.0: TrueType (glyf) outlines.
        assert_eq!(&NOTO_SANS_REGULAR[..4], &[0x00, 0x01, 0x00, 0x00]);
        assert!(
            NOTO_SANS_BOLD_TTF.len() > 100_000,
            "Noto Sans Bold was not embedded"
        );
        assert_eq!(&NOTO_SANS_BOLD_TTF[..4], &[0x00, 0x01, 0x00, 0x00]);
        assert!(
            NOTO_SANS_CJK_TC.len() > 5_000_000,
            "Noto CJK was not embedded"
        );
        // 'OTTO': OpenType with CFF (PostScript) outlines.
        assert_eq!(&NOTO_SANS_CJK_TC[..4], b"OTTO");
    }

    #[test]
    fn the_bold_family_is_registered_bold_first_with_fallbacks() {
        let fonts = {
            let ctx = egui::Context::default();
            install(&ctx);
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
            output.textures_delta.clear();
            ctx.fonts(|f| f.definitions().clone())
        };
        let family = FontFamily::Name(BOLD_FAMILY.into());
        let chain = fonts.families.get(&family).expect("bold family is present");
        assert_eq!(
            chain.first().map(String::as_str),
            Some(NOTO_SANS_BOLD),
            "bold must be tried first"
        );
        assert!(chain.iter().any(|name| name.as_str() == NOTO_SANS));
        assert!(chain.iter().any(|name| name.as_str() == NOTO_SANS_CJK));
    }

    #[test]
    fn install_appends_the_fallbacks_after_the_stocks() {
        let fonts = {
            let ctx = egui::Context::default();
            install(&ctx);
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
            output.textures_delta.clear();
            ctx.fonts(|f| f.definitions().clone())
        };
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            let chain = fonts.families.get(&family).expect("family is present");
            assert!(chain.len() >= 3, "the stock face must stay first");
            assert_eq!(
                chain.get(chain.len() - 2).map(String::as_str),
                Some(NOTO_SANS)
            );
            assert_eq!(chain.last().map(String::as_str), Some(NOTO_SANS_CJK));
        }
        assert!(fonts.font_data.contains_key(NOTO_SANS));
        assert!(fonts.font_data.contains_key(NOTO_SANS_CJK));
    }

    #[test]
    fn the_bundled_cjk_font_covers_cjk_scripts() {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        let font = egui::FontId::proportional(16.0);
        // Traditional-only codepoints, kana, Hangul and Bopomofo.
        let covered = ctx.fonts_mut(|f| f.has_glyphs(&font, "國門說體龍あカ한ㄅ"));
        assert!(covered, "the CJK fallback is not reachable or lacks glyphs");
    }
}
