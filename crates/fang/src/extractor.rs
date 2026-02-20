use scraper::{Html, Selector};
use whatlang::{detect, Lang};
use xxhash_rust::xxh3::xxh3_64;

use crate::text::collapse_whitespace;

const HEADING_TAGS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];

pub fn extract_title(document: &Html) -> String {
    let title_sel = Selector::parse("title").ok();
    let og_title_sel = Selector::parse("meta[property='og:title']").ok();
    let h1_sel = Selector::parse("h1").ok();

    if let Some(sel) = &og_title_sel {
        if let Some(el) = document.select(sel).next() {
            if let Some(content) = el.value().attr("content") {
                let t = collapse_whitespace(content);
                if !t.is_empty() {
                    return t;
                }
            }
        }
    }

    if let Some(sel) = &title_sel {
        if let Some(el) = document.select(sel).next() {
            let text: String = el.text().collect();
            let t = collapse_whitespace(&text);
            if !t.is_empty() {
                return t;
            }
        }
    }

    if let Some(sel) = &h1_sel {
        if let Some(el) = document.select(sel).next() {
            let text: String = el.text().collect();
            let t = collapse_whitespace(&text);
            if !t.is_empty() {
                return t;
            }
        }
    }

    String::new()
}

pub fn extract_meta_description(document: &Html) -> Option<String> {
    let og_sel = Selector::parse("meta[property='og:description']").ok()?;
    let desc_sel = Selector::parse("meta[name='description']").ok()?;

    if let Some(el) = document.select(&og_sel).next() {
        if let Some(content) = el.value().attr("content") {
            let t = collapse_whitespace(content);
            if !t.is_empty() {
                return Some(t);
            }
        }
    }

    if let Some(el) = document.select(&desc_sel).next() {
        if let Some(content) = el.value().attr("content") {
            let t = collapse_whitespace(content);
            if !t.is_empty() {
                return Some(t);
            }
        }
    }

    None
}

pub fn extract_lang(document: &Html, body_text: &str) -> Option<String> {
    let html_sel = Selector::parse("html[lang]").ok()?;

    if let Some(el) = document.select(&html_sel).next() {
        if let Some(lang) = el.value().attr("lang") {
            let l = lang.trim();
            if !l.is_empty() {
                return Some(l.to_lowercase());
            }
        }
    }

    let detect_text = if body_text.len() > 500 {
        let end = body_text.floor_char_boundary(500);
        &body_text[..end]
    } else {
        body_text
    };

    if detect_text.trim().is_empty() {
        return None;
    }

    detect(detect_text).map(|info| lang_to_bcp47(info.lang()))
}

fn lang_to_bcp47(lang: Lang) -> String {
    match lang {
        Lang::Eng => "en",
        Lang::Deu => "de",
        Lang::Fra => "fr",
        Lang::Spa => "es",
        Lang::Por => "pt",
        Lang::Ita => "it",
        Lang::Nld => "nl",
        Lang::Rus => "ru",
        Lang::Cmn => "zh",
        Lang::Jpn => "ja",
        Lang::Kor => "ko",
        Lang::Ara => "ar",
        Lang::Hin => "hi",
        Lang::Tur => "tr",
        Lang::Pol => "pl",
        Lang::Swe => "sv",
        Lang::Dan => "da",
        Lang::Nob => "no",
        Lang::Fin => "fi",
        Lang::Ces => "cs",
        Lang::Slk => "sk",
        Lang::Ron => "ro",
        Lang::Hrv => "hr",
        Lang::Srp => "sr",
        Lang::Bul => "bg",
        Lang::Ukr => "uk",
        Lang::Cat => "ca",
        Lang::Vie => "vi",
        Lang::Tha => "th",
        Lang::Ind => "id",
        _ => "und",
    }
    .to_string()
}

pub fn extract_headings(document: &Html) -> Vec<String> {
    let selector_str = HEADING_TAGS.join(",");
    let sel = match Selector::parse(&selector_str) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    document
        .select(&sel)
        .map(|el| {
            let text: String = el.text().collect();
            collapse_whitespace(&text)
        })
        .filter(|h| !h.is_empty())
        .collect()
}

pub fn compute_content_hash(body: &str) -> u64 {
    xxh3_64(body.as_bytes())
}
