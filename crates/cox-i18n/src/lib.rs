//! `cox-i18n`: the user-facing strings of cox in Fluent (`.ftl`), embedded in
//! the binary, and the one place a message id becomes text in the user's
//! language. English (`en`) is the source and default locale; `ru` and `uk`
//! are translations. A message a translation lacks resolves from `en`, message
//! by message, so a partial translation is safe to ship.
//!
//! Its own crate so every surface (TUI, CLI, the desktop apps through
//! `ftl-export`) reads one set of sources without pulling the Fluent stack
//! into crates that print nothing. No workspace dependency: it is a leaf.
//!
//! [`export`] lowers the same sources to Apple `.strings`/`.stringsdict` and
//! Windows `.resw` files for the native apps (`docs/i18n.md`).

pub mod export;

use std::sync::OnceLock;

use fluent_bundle::FluentResource;
use fluent_bundle::concurrent::FluentBundle;
pub use fluent_bundle::{FluentArgs, FluentValue};
use fluent_langneg::{NegotiationStrategy, negotiate_languages};
pub use unic_langid::LanguageIdentifier;

/// One embedded locale.
#[derive(Debug, Clone, Copy)]
pub struct Locale {
    /// BCP 47 language subtag: the folder under `locales/`, the Fluent
    /// language id (plural rules) and the Apple `<code>.lproj` name.
    pub code: &'static str,
    /// The Windows resource folder (`Strings/<windows>/Resources.resw`).
    pub windows: &'static str,
    /// `locales/<code>/main.ftl`.
    pub source: &'static str,
}

/// The source and fallback locale: every message id exists here first.
pub const DEFAULT_LOCALE: &str = "en";

/// Every shipped locale, the default first. Adding one is a row here plus
/// `locales/<code>/main.ftl` (`locales/README.md`).
pub const LOCALES: &[Locale] = &[
    Locale {
        code: "en",
        windows: "en-US",
        source: include_str!("../locales/en/main.ftl"),
    },
    Locale {
        code: "ru",
        windows: "ru-RU",
        source: include_str!("../locales/ru/main.ftl"),
    },
    Locale {
        code: "uk",
        windows: "uk-UA",
        source: include_str!("../locales/uk/main.ftl"),
    },
];

/// Why a bundle could not be built from the embedded sources.
#[derive(Debug, thiserror::Error)]
pub enum I18nError {
    #[error("locale `{code}` is not a valid BCP 47 language identifier")]
    BadCode { code: String },
    #[error("locale `{code}`: {count} Fluent syntax error(s), first: {first}")]
    Syntax {
        code: String,
        count: usize,
        first: String,
    },
    #[error("locale `{code}`: {detail}")]
    Bundle { code: String, detail: String },
}

type Bundle = FluentBundle<FluentResource>;

/// A negotiated chain of bundles, most preferred first and `en` last.
pub struct Localizer {
    bundles: Vec<(&'static str, Bundle)>,
}

impl Localizer {
    /// Bundles for `requested` (most preferred first) negotiated against
    /// [`LOCALES`], always ending in [`DEFAULT_LOCALE`].
    pub fn new(requested: &[LanguageIdentifier]) -> Result<Self, I18nError> {
        let bundles = negotiate(requested)
            .into_iter()
            .map(|locale| Ok((locale.code, build_bundle(locale)?)))
            .collect::<Result<_, I18nError>>()?;
        Ok(Self { bundles })
    }

    /// [`Localizer::new`] over raw tags as the OS or env spell them
    /// (`uk_UA.UTF-8`, `ru-RU`); tags that do not parse are skipped.
    pub fn for_tags<S: AsRef<str>>(tags: &[S]) -> Result<Self, I18nError> {
        let requested: Vec<_> = tags.iter().filter_map(|t| parse_tag(t.as_ref())).collect();
        Self::new(&requested)
    }

    /// The user's languages from the env, then the OS ([`requested_languages`]).
    pub fn from_env() -> Result<Self, I18nError> {
        Self::new(&requested_languages())
    }

    /// The negotiated locale codes, most preferred first, `en` last.
    pub fn chain(&self) -> Vec<&'static str> {
        self.bundles.iter().map(|(code, _)| *code).collect()
    }

    /// The first locale in the chain that has a value for `id`, formatted.
    /// `None` when no locale, `en` included, defines it.
    pub fn try_format(&self, id: &str, args: Option<&FluentArgs>) -> Option<String> {
        self.bundles.iter().find_map(|(_, bundle)| {
            let pattern = bundle.get_message(id)?.value()?;
            // A formatting error (an argument the caller did not pass) still
            // yields text with the placeable spelled `{$name}`; showing that
            // beats hiding the whole message behind a fallback.
            let mut errors = Vec::new();
            Some(
                bundle
                    .format_pattern(pattern, args, &mut errors)
                    .into_owned(),
            )
        })
    }

    /// [`Localizer::try_format`], or the id itself when nothing defines it, so
    /// a missing string shows up in the UI as its id instead of blank space.
    pub fn format(&self, id: &str, args: Option<&FluentArgs>) -> String {
        self.try_format(id, args).unwrap_or_else(|| id.to_owned())
    }
}

fn build_bundle(locale: &Locale) -> Result<Bundle, I18nError> {
    let langid: LanguageIdentifier = locale.code.parse().map_err(|_| I18nError::BadCode {
        code: locale.code.to_owned(),
    })?;
    let resource = FluentResource::try_new(locale.source.to_owned()).map_err(|(_, errors)| {
        I18nError::Syntax {
            code: locale.code.to_owned(),
            count: errors.len(),
            first: errors.first().map(|e| format!("{e:?}")).unwrap_or_default(),
        }
    })?;
    let mut bundle = Bundle::new_concurrent(vec![langid]);
    // Unicode isolation marks (FSI/PDI) around placeables only matter for
    // right-to-left locales; cox ships none, and the marks break terminal
    // width math and string comparisons in tests.
    bundle.set_use_isolating(false);
    bundle
        .add_resource(resource)
        .map_err(|errors| I18nError::Bundle {
            code: locale.code.to_owned(),
            detail: format!("{errors:?}"),
        })?;
    Ok(bundle)
}

/// Negotiates `requested` against [`LOCALES`] (language match, so `uk-UA`
/// selects `uk`), appending [`DEFAULT_LOCALE`] when it is not already in the
/// chain.
pub fn negotiate(requested: &[LanguageIdentifier]) -> Vec<&'static Locale> {
    let available: Vec<LanguageIdentifier> =
        LOCALES.iter().filter_map(|l| l.code.parse().ok()).collect();
    let default: Option<LanguageIdentifier> = DEFAULT_LOCALE.parse().ok();
    let picked = negotiate_languages(
        requested,
        &available,
        default.as_ref(),
        NegotiationStrategy::Filtering,
    );
    let mut chain: Vec<&'static Locale> = Vec::new();
    let codes = picked
        .iter()
        .map(|id| id.language.as_str())
        .chain([DEFAULT_LOCALE]);
    for code in codes {
        if let Some(locale) = LOCALES.iter().find(|l| l.code == code)
            && !chain.iter().any(|l| l.code == code)
        {
            chain.push(locale);
        }
    }
    chain
}

/// Parses a tag as POSIX env vars and OS APIs spell it: `uk_UA.UTF-8`,
/// `ru_RU@euro`, `en-US`. `C` and `POSIX` carry no language and give `None`.
pub fn parse_tag(raw: &str) -> Option<LanguageIdentifier> {
    let tag = raw.split(['.', '@']).next()?.trim().replace('_', "-");
    if tag.is_empty() || tag.eq_ignore_ascii_case("c") || tag.eq_ignore_ascii_case("posix") {
        return None;
    }
    tag.parse().ok()
}

/// The user's languages, most preferred first: the POSIX message locale
/// (`LC_ALL`, `LC_MESSAGES`, `LANG`, first one set wins), then the OS UI
/// languages (macOS and Windows preferences; a GUI app gets no `LANG`).
pub fn requested_languages() -> Vec<LanguageIdentifier> {
    requested_from(|name| std::env::var(name).ok(), sys_locale::get_locales())
}

fn requested_from(
    env: impl Fn(&str) -> Option<String>,
    os: impl IntoIterator<Item = String>,
) -> Vec<LanguageIdentifier> {
    let posix = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|name| env(name).filter(|v| !v.is_empty()));
    posix
        .into_iter()
        .chain(os)
        .filter_map(|tag| parse_tag(&tag))
        .collect()
}

/// The process-wide localizer, negotiated from the env and OS on first use.
/// If the embedded sources were broken (the tests below rule that out) it
/// degrades to an empty chain, where every message renders as its id.
pub fn global() -> &'static Localizer {
    static GLOBAL: OnceLock<Localizer> = OnceLock::new();
    GLOBAL.get_or_init(|| {
        Localizer::from_env().unwrap_or(Localizer {
            bundles: Vec::new(),
        })
    })
}

/// `id` in the user's language through [`global`]; see [`Localizer::format`].
pub fn t(id: &str, args: Option<&FluentArgs>) -> String {
    global().format(id, args)
}

/// `tr!("id")` or `tr!("id", name = value, …)`: [`t`] with the named arguments
/// set as Fluent variables (`$name`). Values are anything `Into<FluentValue>`;
/// pass numbers as numbers so plural selection sees them.
#[macro_export]
macro_rules! tr {
    ($id:expr $(,)?) => {
        $crate::t($id, None)
    };
    ($id:expr, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = $crate::FluentArgs::new();
        $(args.set(stringify!($name), $value);)+
        $crate::t($id, Some(&args))
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    fn localizer(tag: &str) -> Localizer {
        Localizer::for_tags(&[tag]).expect("embedded locales build")
    }

    fn sessions(l: &Localizer, count: i64) -> String {
        let mut args = FluentArgs::new();
        args.set("count", count);
        l.format("session-count", Some(&args))
    }

    #[test]
    fn every_embedded_locale_builds() {
        for locale in LOCALES {
            build_bundle(locale).unwrap_or_else(|e| panic!("{e}"));
        }
    }

    #[test]
    fn russian_plurals_pick_one_few_many() {
        let ru = localizer("ru");
        assert_eq!(sessions(&ru, 1), "1 сессия");
        assert_eq!(sessions(&ru, 2), "2 сессии");
        assert_eq!(sessions(&ru, 5), "5 сессий");
        assert_eq!(sessions(&ru, 21), "21 сессия");
    }

    #[test]
    fn ukrainian_plurals_pick_one_few_many() {
        let uk = localizer("uk");
        assert_eq!(sessions(&uk, 1), "1 сесія");
        assert_eq!(sessions(&uk, 2), "2 сесії");
        assert_eq!(sessions(&uk, 5), "5 сесій");
        assert_eq!(sessions(&uk, 21), "21 сесія");
    }

    #[test]
    fn english_plurals_pick_one_other() {
        let en = localizer("en");
        assert_eq!(sessions(&en, 1), "1 session");
        assert_eq!(sessions(&en, 2), "2 sessions");
        assert_eq!(sessions(&en, 21), "21 sessions");
    }

    #[test]
    fn unsupported_locale_falls_back_to_english() {
        let de = localizer("de-DE");
        assert_eq!(de.chain(), ["en"]);
        assert_eq!(de.format("settings-title", None), "Settings");
    }

    #[test]
    fn message_missing_from_a_translation_resolves_from_english() {
        for tag in ["ru", "uk"] {
            let l = localizer(tag);
            assert_eq!(l.chain(), [tag, "en"]);
            assert_ne!(l.format("settings-title", None), "Settings");
            assert_eq!(l.format("send-feedback", None), "Send feedback");
        }
    }

    #[test]
    fn missing_key_renders_as_its_id() {
        let ru = localizer("ru");
        assert_eq!(ru.try_format("no-such-message", None), None);
        assert_eq!(ru.format("no-such-message", None), "no-such-message");
    }

    #[test]
    fn os_and_posix_tags_select_the_language() {
        assert_eq!(localizer("uk-UA").chain(), ["uk", "en"]);
        assert_eq!(localizer("uk_UA.UTF-8").chain(), ["uk", "en"]);
        assert_eq!(localizer("ru_RU@euro").chain(), ["ru", "en"]);
        assert_eq!(localizer("C").chain(), ["en"]);
    }

    #[test]
    fn posix_env_wins_over_os_languages() {
        let env = |name: &str| (name == "LANG").then(|| "uk_UA.UTF-8".to_owned());
        let requested = requested_from(env, ["ru-RU".to_owned()]);
        let chain = Localizer::new(&requested).expect("builds").chain();
        assert_eq!(chain, ["uk", "ru", "en"]);
    }

    #[test]
    fn terms_and_variables_are_substituted() {
        let mut args = FluentArgs::new();
        args.set("name", "Ivan");
        assert_eq!(
            localizer("uk").format("welcome-user", Some(&args)),
            "Ласкаво просимо до Cox, Ivan!"
        );
    }

    #[test]
    fn translations_define_no_id_english_lacks() {
        let en = localizer("en");
        for locale in LOCALES {
            let bundle = build_bundle(locale).expect("builds");
            let resource = fluent_syntax::parser::parse(locale.source).expect("parses");
            for entry in resource.body {
                if let fluent_syntax::ast::Entry::Message(m) = entry {
                    assert!(bundle.has_message(m.id.name));
                    assert!(
                        en.try_format(m.id.name, None).is_some(),
                        "{}: `{}` is not in en",
                        locale.code,
                        m.id.name
                    );
                }
            }
        }
    }

    #[test]
    fn tr_macro_passes_named_arguments() {
        // The global chain depends on the machine's locale; English is always
        // in it, so check the fallback-only message and argument plumbing.
        assert_eq!(tr!("send-feedback"), global().format("send-feedback", None));
        let text = tr!("session-count", count = 3);
        assert!(text.starts_with('3'), "{text}");
    }
}
