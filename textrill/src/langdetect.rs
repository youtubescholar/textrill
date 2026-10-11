//! Script classification and language-run wrapping.
//!
//! `--lang` declares the document's language and lands on the root `<html>`;
//! `--lang_runs` additionally wraps runs of a *different* script in
//! `<span lang="…">`. This module does the run side. It is deliberately a
//! script classifier, not a language detector:
//!
//! - The CJCK triad is decided categorically by script membership: kana →
//!   `ja`, hangul → `ko`, Han with neither → `zh`. Japanese prose mixes
//!   kanji and kana, so a run is grouped across Han + kana + hangul and the
//!   final code is decided by what it contains.
//! - Scripts whose dominant written language is a strong, well-known majority
//!   map to one code (Devanagari → `hi`, Ethiopic → `am`, Tamil → `ta`, …).
//! - Scripts that serve several languages are *not* tagged (Latin, Cyrillic,
//!   Arabic script, Canadian Aboriginal Syllabics): tagging them would assert
//!   a language the script cannot evidence. They break runs instead.
//!
//! The dominant language declared with `--lang` is compared by its primary
//! subtag (`zh-Hans` compares as `zh`), so a document tagged as its own
//! dominant language is never re-wrapped.

#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Text in the declared/dominant script; breaks runs, never wrapped.
    Latin,
    /// Han ideographs, kana, hangul and CJK punctuation/fullwidth forms.
    Cjk,
    /// A script mapped to one dominant language code.
    Single(&'static str),
    /// An alphabetic script we deliberately do not tag; breaks runs.
    Other,
    /// Whitespace, digits and punctuation; bridges inside a run.
    Neutral,
}

fn kind(c: char) -> Kind {
    if is_cjk(c) {
        return Kind::Cjk;
    }
    if let Some(lang) = single_script_lang(c) {
        return Kind::Single(lang);
    }
    if is_latin_letter(c) {
        return Kind::Latin;
    }
    if c.is_whitespace() || !c.is_alphabetic() {
        return Kind::Neutral;
    }
    Kind::Other
}

/// Han ideographs (BMP and the extensions that matter), kana, hangul and the
/// CJCK punctuation/fullwidth blocks.
fn is_cjk(c: char) -> bool {
    is_han(c)
        || is_kana(c)
        || is_hangul(c)
        || matches!(c, '\u{3000}'..='\u{303F}' | '\u{FF00}'..='\u{FFEF}')
}

fn is_han(c: char) -> bool {
    matches!(c,
        '\u{3400}'..='\u{4DBF}'  // CJK Ext A
        | '\u{4E00}'..='\u{9FFF}' // Unified
        | '\u{F900}'..='\u{FAFF}' // Compatibility
        | '\u{20000}'..='\u{2A6DF}') // Ext B (real CJK text can live past the BMP)
}

fn is_kana(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{FF65}'..='\u{FF9F}') // Half-width katakana
}

fn is_hangul(c: char) -> bool {
    matches!(c,
        '\u{1100}'..='\u{11FF}' // Jamo
        | '\u{3130}'..='\u{318F}' // Compatibility jamo
        | '\u{AC00}'..='\u{D7A3}') // Syllables
}

/// One character of a script whose written language is a strong, well-known
/// majority (each guessed language noted; the map is a documented contract).
fn single_script_lang(c: char) -> Option<&'static str> {
    match c {
        // Brahmic family, each mapped to its most widely spoken language.
        '\u{0900}'..='\u{097F}' => Some("hi"), // Devanagari (Hindi; also mr/ne/sa)
        '\u{0980}'..='\u{09FF}' => Some("bn"), // Bengali (also as)
        '\u{0A00}'..='\u{0A7F}' => Some("pa"), // Gurmukhi
        '\u{0A80}'..='\u{0AFF}' => Some("gu"), // Gujarati
        '\u{0B80}'..='\u{0BFF}' => Some("ta"), // Tamil
        '\u{0C00}'..='\u{0C7F}' => Some("te"), // Telugu
        '\u{0C80}'..='\u{0CFF}' => Some("kn"), // Kannada
        '\u{0D00}'..='\u{0D7F}' => Some("ml"), // Malayalam
        '\u{0D80}'..='\u{0DFF}' => Some("si"), // Sinhala
        // South-East Asia: each script maps closely to one language.
        '\u{0E00}'..='\u{0E7F}' => Some("th"), // Thai
        '\u{0E80}'..='\u{0EFF}' => Some("lo"), // Lao
        '\u{1000}'..='\u{109F}' => Some("my"), // Myanmar/Burmese
        // Others with a single dominant written language.
        '\u{0530}'..='\u{058F}' => Some("hy"),  // Armenian
        '\u{0590}'..='\u{05FF}' => Some("he"),  // Hebrew
        '\u{0370}'..='\u{03FF}' => Some("el"),  // Greek
        '\u{10A0}'..='\u{10FF}' => Some("ka"),  // Georgian
        '\u{1200}'..='\u{137F}' => Some("am"),  // Ethiopic (Amharic; also ti)
        '\u{13A0}'..='\u{13FF}' => Some("chr"), // Cherokee
        '\u{0F00}'..='\u{0FFF}' => Some("bo"),  // Tibetan
        '\u{1800}'..='\u{18AF}' => Some("mn"),  // Mongolian
        _ => None,
    }
}

fn is_latin_letter(c: char) -> bool {
    c.is_ascii_alphabetic()
        || matches!(c,
            '\u{00C0}'..='\u{00FF}'
            | '\u{0100}'..='\u{017F}'
            | '\u{0180}'..='\u{024F}'
            | '\u{1E00}'..='\u{1EFF}')
}

/// The primary language subtag of a BCP 47 tag, lowercased: `zh-Hans` → `zh`,
/// `EN-us` → `en`.
fn primary(lang: &str) -> String {
    lang.split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// The language code to tag `run` with: `Some` for a script we can name,
/// `None` for one we refuse to guess.
fn decided(run: &Run) -> &'static str {
    match run.lang {
        Some(lang) => lang,
        None if run.had_kana => "ja",
        None if run.had_hangul => "ko",
        None => "zh",
    }
}

#[derive(Debug)]
struct Run {
    lang: Option<&'static str>,
    buffer: String,
    had_kana: bool,
    had_hangul: bool,
}

impl Run {
    fn is_cjk(&self) -> bool {
        self.lang.is_none()
    }
}

/// Does `c` continue the open `run`, after a streak of neutral characters?
/// A CJK run continues with CJK; a single-script run with the same script.
fn run_continues(run: &Run, c: char) -> bool {
    if run.is_cjk() {
        return is_cjk(c);
    }
    single_script_lang(c) == run.lang
}

/// A tag-aware pass over already-escaped HTML. Tags (from any `<` to its `>`)
/// are copied verbatim; text runs of a script different from the declared
/// dominant language are wrapped in `<span lang="…">`. Neutral characters
/// inside a run — spaces and punctuation — stay in the run only when the next
/// non-neutral character continues it, so trailing punctuation is not pulled
/// inside a span. Unmapped scripts and the dominant script break runs and are
/// left alone.
pub fn wrap_runs(text: &str, lang: &str) -> String {
    let dominant = primary(lang);
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(text.len() + 8);
    let mut run: Option<Run> = None;
    let mut i = 0usize;

    macro_rules! flush {
        () => {
            if let Some(r) = run.take() {
                let code = decided(&r);
                if primary(code) != dominant {
                    out.push_str("<span lang=\"");
                    out.push_str(code);
                    out.push_str("\">");
                    out.push_str(&r.buffer);
                    out.push_str("</span>");
                } else {
                    out.push_str(&r.buffer);
                }
            }
        };
    }

    while i < n {
        let c = chars[i];
        if c == '<' {
            flush!();
            let start = i;
            while i < n && chars[i] != '>' {
                i += 1;
            }
            i += 1;
            for &ch in &chars[start..i.min(n)] {
                out.push(ch);
            }
            continue;
        }
        let mut advanced = false;
        match kind(c) {
            Kind::Cjk => {
                let need_new = match &mut run {
                    None => true,
                    Some(r) if !r.is_cjk() => {
                        flush!();
                        true
                    }
                    Some(r) => {
                        r.buffer.push(c);
                        r.had_kana |= is_kana(c);
                        r.had_hangul |= is_hangul(c);
                        false
                    }
                };
                if need_new {
                    run = Some(Run {
                        lang: None,
                        buffer: c.to_string(),
                        had_kana: is_kana(c),
                        had_hangul: is_hangul(c),
                    });
                }
            }
            Kind::Single(_) => {
                let need_new = match &mut run {
                    None => true,
                    Some(r) if single_script_lang(c) != r.lang => {
                        flush!();
                        true
                    }
                    Some(r) => {
                        r.buffer.push(c);
                        false
                    }
                };
                if need_new {
                    run = Some(Run {
                        lang: single_script_lang(c),
                        buffer: c.to_string(),
                        had_kana: false,
                        had_hangul: false,
                    });
                }
            }
            Kind::Neutral => match &mut run {
                None => out.push(c),
                Some(r) => {
                    // Look past the streak of neutrals; bridge into the run
                    // only if what follows continues it.
                    let mut j = i;
                    while j < n && kind(chars[j]) == Kind::Neutral {
                        j += 1;
                    }
                    let continues = j < n && run_continues(r, chars[j]);
                    if continues {
                        for &nc in &chars[i..j] {
                            r.buffer.push(nc);
                        }
                        i = j;
                        advanced = true;
                    } else {
                        flush!();
                        for &nc in &chars[i..j] {
                            out.push(nc);
                        }
                        i = j;
                        advanced = true;
                    }
                }
            },
            Kind::Latin | Kind::Other => {
                flush!();
                out.push(c);
            }
        }
        if !advanced {
            i += 1;
        }
    }
    flush!();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_document_wraps_a_chinese_passage() {
        assert_eq!(
            wrap_runs("Hello 世界 World", "en"),
            "Hello <span lang=\"zh\">世界</span> World"
        );
    }

    #[test]
    fn spaces_between_cjk_bridge_into_one_span() {
        assert_eq!(
            wrap_runs("中文 继续", "en"),
            "<span lang=\"zh\">中文 继续</span>"
        );
    }

    #[test]
    fn trailing_space_stays_outside_the_span() {
        assert_eq!(
            wrap_runs("A中文 B", "en"),
            "A<span lang=\"zh\">中文</span> B"
        );
    }

    #[test]
    fn japanese_kanji_and_kana_are_one_span() {
        // Kana presence decides the whole Han+kana run as Japanese.
        assert_eq!(
            wrap_runs("これはテストです", "en"),
            "<span lang=\"ja\">これはテストです</span>"
        );
    }

    #[test]
    fn hangul_is_tagged_korean() {
        assert_eq!(
            wrap_runs("안녕하세요", "en"),
            "<span lang=\"ko\">안녕하세요</span>"
        );
    }

    #[test]
    fn the_dominant_language_is_not_rewrapped() {
        assert_eq!(wrap_runs("日本語のテキスト", "ja"), "日本語のテキスト");
        // Primary subtags compare: zh-Hans matches zh.
        assert_eq!(wrap_runs("中文", "zh-Hans"), "中文");
    }

    #[test]
    fn a_chinese_passage_inside_a_japanese_document_is_wrapped() {
        // Bare Han with no kana in a ja document reads as a foreign passage.
        assert_eq!(wrap_runs("中国語", "ja"), "<span lang=\"zh\">中国語</span>");
    }

    #[test]
    fn an_unmapped_script_breaks_runs_without_tagging() {
        // Cyrillic is an untagged script: it breaks text but is never wrapped,
        // leaving the CJK passage that follows it wrapped.
        assert_eq!(
            wrap_runs("Привет 中文", "en"),
            "Привет <span lang=\"zh\">中文</span>"
        );
    }

    #[test]
    fn tags_are_copied_verbatim() {
        assert_eq!(
            wrap_runs("<a href=\"x.html\">中文</a>", "en"),
            "<a href=\"x.html\"><span lang=\"zh\">中文</span></a>"
        );
    }

    #[test]
    fn devanagari_maps_to_hindi() {
        assert_eq!(
            wrap_runs("नमस्ते दुनिया", "en"),
            "<span lang=\"hi\">नमस्ते दुनिया</span>"
        );
    }

    #[test]
    fn empty_lang_wraps_every_mapped_script() {
        assert_eq!(wrap_runs("中文", ""), "<span lang=\"zh\">中文</span>");
    }

    #[test]
    fn primary_subtag_extraction() {
        assert_eq!(primary("zh-Hans"), "zh");
        assert_eq!(primary("EN_us"), "en");
        assert_eq!(primary(""), "");
    }
}
