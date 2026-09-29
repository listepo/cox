# Locales

Fluent (`.ftl`) sources for `cox-i18n`. `en` is the source and fallback locale;
`ru` and `uk` are translations. See `docs/i18n.md` for the full guide.

## Adding a locale

1. Create `locales/<code>/main.ftl`, where `<code>` is the BCP 47 language
   subtag (`de`, `pl`), not a country code.
2. Translate the messages from `en/main.ftl`. A message you leave out resolves
   from `en`, so a partial translation is safe to ship.
3. Register the locale in `LOCALES` in `crates/cox-i18n/src/lib.rs` (the code,
   its `include_str!`, the Windows resource folder such as `de-DE`).
4. Use the plural categories CLDR defines for the language (`one`/`other` for
   English and German, `one`/`few`/`many`/`other` for Russian and Ukrainian,
   and so on), always with a `*[other]` default.
5. Run `cargo test -p cox-i18n` and `cargo run -p cox-i18n --bin ftl-export`.
