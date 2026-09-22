//! Which recognition language a read asks for, on a platform that names its languages its own
//! way.
//!
//! A module writes a BCP 47 tag — `"de"`, `"de-DE"`, `"zh-Hant"` — or a list of them in order
//! of preference, or nothing. Windows lists the OCR languages that are installed
//! (`"de-DE"`, `"en-US"`), and Vision lists what that macOS version reads (`"de-DE"`,
//! `"zh-Hans"`). Matching the one against the other is language negotiation, and that is
//! `fluent-langneg` with its CLDR likely-subtags data: `"de"` finds `"de-DE"`, `"zh-TW"` finds
//! `"zh-Hant"`, `"de-CH"` finds `"de-DE"`. What comes back is the platform's OWN string, the one
//! its engine was listed under, so the engine is always handed a name from its own list.
//!
//! One thing the crate does not do, and this file does: refuse a tag that is not one. The
//! crate's own converter drops malformed tags without a word, which would turn a typo into
//! "the user's language" — so a tag is parsed here first, and a bad one is an error the
//! binding raises.
//!
//! Std and `fluent-langneg` only; borrowed by `crates/macos-check`.

use fluent_langneg::{negotiate_languages, LanguageIdentifier, NegotiationStrategy};

/// What a read asked for, as the module wrote it (trimmed): nothing, or tags in order of
/// preference. Part of a job's identity, so two reads asking the same thing share a capture.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum LangReq {
    #[default]
    Default,
    Tags(Vec<String>),
}

/// The languages the platform offers, read once by the recognise thread and published.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Languages {
    /// What the engine reads, in the platform's order and spelling.
    pub available: Vec<String>,
    /// The subset its fast model reads (macOS; the same list elsewhere).
    pub fast: Vec<String>,
    /// The user's preferred languages, most preferred first, as the system spells them.
    pub preferred: Vec<String>,
}

/// Parses one tag, refusing what is not one — including `"und"`, which names no language.
pub fn parse(tag: &str) -> Result<LanguageIdentifier, String> {
    let t = tag.trim();
    let bad = || {
        format!(
            "'{tag}' is not a language tag — write it like \"de\", \"de-DE\" or \"zh-Hant\""
        )
    };
    if t.is_empty() {
        return Err(bad());
    }
    let id = LanguageIdentifier::try_from_bytes(t.as_bytes()).map_err(|_| bad())?;
    if id.language.is_empty() {
        return Err(bad());
    }
    Ok(id)
}

/// Checks every tag of a request, and returns it trimmed.
pub fn validate(req: LangReq) -> Result<LangReq, String> {
    match req {
        LangReq::Default => Ok(LangReq::Default),
        LangReq::Tags(tags) => {
            if tags.is_empty() {
                return Err("an empty list of languages asks for nothing — leave `lang` out for \
                            the user's language"
                    .to_string());
            }
            let mut out = Vec::with_capacity(tags.len());
            for t in tags {
                parse(&t)?;
                out.push(t.trim().to_string());
            }
            Ok(LangReq::Tags(out))
        }
    }
}

/// The platform's tags that parse, beside their parsed form, in the platform's order.
fn parsed(list: &[String]) -> (Vec<&String>, Vec<LanguageIdentifier>) {
    let mut names = Vec::new();
    let mut ids = Vec::new();
    for s in list {
        if let Ok(id) = LanguageIdentifier::try_from_bytes(s.trim().as_bytes()) {
            names.push(s);
            ids.push(id);
        }
    }
    (names, ids)
}

/// The best entry of `available` for one requested tag, as the platform spells it.
fn best_for(requested: &LanguageIdentifier, available: &[String]) -> Option<String> {
    let (names, ids) = parsed(available);
    let found = negotiate_languages(
        std::slice::from_ref(requested),
        &ids,
        None,
        NegotiationStrategy::Lookup,
    );
    let hit = found.first()?;
    let i = ids.iter().position(|id| std::ptr::eq(id, *hit))?;
    Some(names[i].clone())
}

/// What a read with no `lang` uses: the first of the user's preferred languages that the
/// engine reads, then English, then whatever the engine lists first.
pub fn default_tag(langs: &Languages) -> Option<String> {
    for p in &langs.preferred {
        if let Ok(id) = parse(p) {
            if let Some(hit) = best_for(&id, &langs.available) {
                return Some(hit);
            }
        }
    }
    if let Ok(en) = parse("en") {
        if let Some(hit) = best_for(&en, &langs.available) {
            return Some(hit);
        }
    }
    langs.available.first().cloned()
}

/// The platform tag `req` resolves to, or the sentence a failed reading carries. Tags are
/// tried in order; the first that the engine reads wins.
pub fn resolve(req: &LangReq, langs: &Languages) -> Result<String, String> {
    match req {
        LangReq::Default => default_tag(langs).ok_or_else(|| {
            format!("language: no text recognition language is available here. {}", remedy())
        }),
        LangReq::Tags(tags) => {
            for t in tags {
                // A malformed tag is refused by the binding before it gets here; one that
                // slipped through simply matches nothing.
                if let Ok(id) = parse(t) {
                    if let Some(hit) = best_for(&id, &langs.available) {
                        return Ok(hit);
                    }
                }
            }
            Err(unavailable(tags, langs))
        }
    }
}

/// The sentence for a request nothing on this machine reads.
pub fn unavailable(tags: &[String], langs: &Languages) -> String {
    let asked = if tags.len() == 1 {
        format!("{} is not available here", tags[0])
    } else {
        format!("none of {} is available here", tags.join(", "))
    };
    let have = if langs.available.is_empty() {
        "nothing is".to_string()
    } else {
        langs.available.join(", ")
    };
    format!("language: {asked} (available: {have}). {}", remedy())
}

/// What the user can do about a missing language, per platform.
pub fn remedy() -> &'static str {
    if cfg!(windows) {
        "Windows reads a language once its optical character recognition component is \
         installed: Settings, Time & language, Language & region, add the language"
    } else if cfg!(target_os = "macos") {
        "Vision on this version of macOS does not read it"
    } else {
        "there is no text recogniser on this platform"
    }
}

/// Every language the engine reads, the one a read without `lang` uses first.
pub fn listed(langs: &Languages) -> Vec<String> {
    let mut out = Vec::with_capacity(langs.available.len());
    if let Some(d) = default_tag(langs) {
        out.push(d);
    }
    for a in &langs.available {
        if !out.contains(a) {
            out.push(a.clone());
        }
    }
    out
}

/// Whether the fast model reads `tag` too.
pub fn fast_reads(tag: &str, langs: &Languages) -> bool {
    langs.fast.iter().any(|f| f == tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn langs(available: &[&str], preferred: &[&str]) -> Languages {
        Languages {
            available: available.iter().map(|s| s.to_string()).collect(),
            fast: available.iter().map(|s| s.to_string()).collect(),
            preferred: preferred.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn tags(t: &[&str]) -> LangReq {
        LangReq::Tags(t.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn a_tag_that_is_not_one_is_refused_rather_than_dropped() {
        for bad in ["", "   ", "english", "de--DE", "12", "und", "de-DE-u-co-phonebk", "d"] {
            assert!(parse(bad).is_err(), "{bad:?} parsed");
        }
        for good in ["de", "DE", "de-DE", "de_DE", "zh-Hant", "zh-Hant-TW", " en-us "] {
            assert!(parse(good).is_ok(), "{good:?} refused");
        }
        assert!(validate(tags(&["de", "klingonisch-ish-no"])).is_err());
        assert!(validate(LangReq::Tags(Vec::new())).is_err());
        assert_eq!(validate(tags(&[" de "])), Ok(tags(&["de"])));
    }

    /// Tesseract's three-letter codes are well-formed tags (a language subtag has two or three
    /// letters), so they do not raise — and no platform lists a language under them, so they
    /// match nothing and are answered as unavailable. What ocr.md says about `"eng"`.
    #[test]
    fn a_three_letter_code_is_a_tag_that_matches_nothing() {
        let l = langs(&["de-DE", "en-US"], &[]);
        for code in ["eng", "deu"] {
            assert!(parse(code).is_ok(), "{code} refused");
            let e = resolve(&tags(&[code]), &l).unwrap_err();
            assert!(e.starts_with(&format!("language: {code} is not available here (available: de-DE, en-US)")), "{e}");
        }
    }

    /// The platform's own spelling comes back, whatever the request looked like.
    #[test]
    fn exact_primary_only_case_and_underscore_all_find_the_platform_tag() {
        let l = langs(&["en-US", "de-DE"], &[]);
        for asked in ["de-DE", "de", "DE", "de_DE", "de-de"] {
            assert_eq!(resolve(&tags(&[asked]), &l), Ok("de-DE".to_string()), "{asked}");
        }
        // A region the engine does not list still reads with the language it does.
        assert_eq!(resolve(&tags(&["de-CH"]), &l), Ok("de-DE".to_string()));
        assert_eq!(resolve(&tags(&["en-GB"]), &l), Ok("en-US".to_string()));
    }

    #[test]
    fn chinese_regions_find_their_script() {
        let l = langs(&["en-US", "zh-Hans", "zh-Hant"], &[]);
        assert_eq!(resolve(&tags(&["zh-CN"]), &l), Ok("zh-Hans".to_string()));
        assert_eq!(resolve(&tags(&["zh-SG"]), &l), Ok("zh-Hans".to_string()));
        assert_eq!(resolve(&tags(&["zh-TW"]), &l), Ok("zh-Hant".to_string()));
        assert_eq!(resolve(&tags(&["zh-HK"]), &l), Ok("zh-Hant".to_string()));
        assert_eq!(resolve(&tags(&["zh"]), &l), Ok("zh-Hans".to_string()));
    }

    #[test]
    fn a_bare_language_prefers_its_default_region() {
        let cases = [
            (&["de-AT", "de-DE", "de-CH"][..], "de", "de-DE"),
            (&["en-GB", "en-US"][..], "en", "en-US"),
            (&["fr-CA", "fr-FR"][..], "fr", "fr-FR"),
            (&["es-MX", "es-ES"][..], "es", "es-ES"),
            (&["pt-PT", "pt-BR"][..], "pt", "pt-BR"),
            (&["it-CH", "it-IT"][..], "it", "it-IT"),
        ];
        for (available, asked, want) in cases {
            let l = langs(available, &[]);
            assert_eq!(resolve(&tags(&[asked]), &l), Ok(want.to_string()), "{asked} in {available:?}");
        }
        // Without the default region, another region of the language still does.
        assert_eq!(resolve(&tags(&["en"]), &langs(&["en-GB"], &[])), Ok("en-GB".to_string()));
    }

    #[test]
    fn a_list_is_tried_in_order_and_an_unavailable_one_says_what_is_there() {
        let l = langs(&["en-US", "fr-FR"], &[]);
        assert_eq!(resolve(&tags(&["de", "fr", "en"]), &l), Ok("fr-FR".to_string()));
        let e = resolve(&tags(&["de", "ja"]), &l).unwrap_err();
        assert!(e.starts_with("language: none of de, ja is available here (available: en-US, fr-FR)"), "{e}");
        let e = resolve(&tags(&["de"]), &l).unwrap_err();
        assert!(e.starts_with("language: de is not available here"), "{e}");
    }

    #[test]
    fn no_lang_means_the_users_language_then_english_then_the_first_listed() {
        let l = langs(&["en-US", "de-DE"], &["de-AT", "en-US"]);
        assert_eq!(resolve(&LangReq::Default, &l), Ok("de-DE".to_string()));
        // A preferred language the engine does not read is passed over for the next one.
        let l = langs(&["en-US", "de-DE"], &["ja-JP", "de"]);
        assert_eq!(resolve(&LangReq::Default, &l), Ok("de-DE".to_string()));
        let l = langs(&["fr-FR", "en-GB"], &["ja-JP"]);
        assert_eq!(resolve(&LangReq::Default, &l), Ok("en-GB".to_string()));
        let l = langs(&["fr-FR", "it-IT"], &["ja-JP"]);
        assert_eq!(resolve(&LangReq::Default, &l), Ok("fr-FR".to_string()));
        assert!(resolve(&LangReq::Default, &langs(&[], &["de"])).unwrap_err().starts_with("language: no text"));
        // An unparseable preferred entry is skipped, not fatal.
        let l = langs(&["en-US"], &["??", "en"]);
        assert_eq!(default_tag(&l), Some("en-US".to_string()));
    }

    #[test]
    fn the_list_puts_the_default_first_and_names_each_language_once() {
        let l = langs(&["en-US", "de-DE", "fr-FR"], &["de"]);
        assert_eq!(listed(&l), vec!["de-DE", "en-US", "fr-FR"]);
        assert!(listed(&langs(&[], &["de"])).is_empty());
        assert!(fast_reads("de-DE", &l));
        assert!(!fast_reads("de", &l));
    }
}
