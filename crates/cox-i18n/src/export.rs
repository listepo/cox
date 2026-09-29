//! Lowers the embedded Fluent sources to the native apps' resource formats:
//! Apple `<code>.lproj/Localizable.strings` (plain messages) and
//! `Localizable.stringsdict` (plural selects) for `desktop/macos`, and Windows
//! `Strings/<tag>/Resources.resw` for the planned WinUI app (`desktop/windows`).
//!
//! Separate from the runtime because it reads the Fluent AST (`fluent-syntax`)
//! rather than formatting messages, and because the formats have limits the
//! runtime does not: the subset it accepts is documented in `docs/i18n.md`.
//! Messages a translation lacks are filled from `en`, so the native apps see
//! the same per-message fallback as the Rust side.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use fluent_syntax::ast;

use crate::{DEFAULT_LOCALE, LOCALES, Locale};

/// The CLDR plural categories a variant key may name.
const CATEGORIES: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("locale `{code}`: Fluent syntax error: {detail}")]
    Syntax { code: String, detail: String },
    #[error("locale `{code}`, message `{id}`: {reason}")]
    Unsupported {
        code: String,
        id: String,
        reason: String,
    },
    #[error("writing {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// A piece of a lowered pattern.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Var(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Shape {
    Simple(Vec<Piece>),
    /// One select on `selector`, with text around it. Variants keep source
    /// order and name a CLDR category each.
    Plural {
        selector: String,
        prefix: Vec<Piece>,
        variants: Vec<(String, Vec<Piece>)>,
        suffix: Vec<Piece>,
    },
}

#[derive(Debug, Clone)]
struct Message {
    id: String,
    shape: Shape,
    comment: Option<String>,
    /// Filled from `en` because the locale lacks it.
    fallback: bool,
}

/// One locale ready to render, messages in `en` source order.
struct Lowered {
    locale: &'static Locale,
    messages: Vec<Message>,
    numeric: Vec<String>,
}

/// Writes every locale under `out`: `apple/<code>.lproj/…` and
/// `windows/Strings/<tag>/Resources.resw`. Returns the files written.
pub fn export_all(out: &Path) -> Result<Vec<PathBuf>, ExportError> {
    let lowered = lower_all()?;
    let en = lowered
        .iter()
        .find(|l| l.locale.code == DEFAULT_LOCALE)
        .map(|l| &l.messages);
    let order = en.map(|m| arg_order(m)).unwrap_or_default();
    let mut written = Vec::new();
    for l in &lowered {
        let lproj = out.join("apple").join(format!("{}.lproj", l.locale.code));
        let strings = render_strings(&l.messages, &order, &l.numeric);
        written.push(write(&lproj.join("Localizable.strings"), &strings)?);
        let dict = render_stringsdict(&l.messages, &order, &l.numeric);
        written.push(write(&lproj.join("Localizable.stringsdict"), &dict)?);
        let resw_dir = out.join("windows").join("Strings").join(l.locale.windows);
        let resw = render_resw(&l.messages, &order);
        written.push(write(&resw_dir.join("Resources.resw"), &resw)?);
    }
    Ok(written)
}

fn write(path: &Path, contents: &str) -> Result<PathBuf, ExportError> {
    let io = |source| ExportError::Io {
        path: path.to_owned(),
        source,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    std::fs::write(path, contents).map_err(io)?;
    Ok(path.to_owned())
}

/// Lowers every locale and fills each translation's gaps from `en`.
fn lower_all() -> Result<Vec<Lowered>, ExportError> {
    let mut per_locale = Vec::new();
    for locale in LOCALES {
        per_locale.push((locale, lower_source(locale.code, locale.source)?));
    }
    let en: Vec<Message> = per_locale
        .iter()
        .find(|(l, _)| l.code == DEFAULT_LOCALE)
        .map(|(_, m)| m.clone())
        .unwrap_or_default();
    let en_vars = arg_order(&en);
    let mut out = Vec::new();
    for (locale, messages) in per_locale {
        let mut by_id: HashMap<String, Message> =
            messages.into_iter().map(|m| (m.id.clone(), m)).collect();
        let mut filled = Vec::new();
        for source in &en {
            match by_id.remove(&source.id) {
                Some(m) => filled.push(m),
                None => filled.push(Message {
                    fallback: true,
                    ..source.clone()
                }),
            }
        }
        if let Some(extra) = by_id.into_keys().next() {
            return Err(unsupported(
                locale.code,
                &extra,
                "the id is not defined in en",
            ));
        }
        for m in &filled {
            let allowed = en_vars.get(&m.id).cloned().unwrap_or_default();
            if let Some(v) = vars(&m.shape).into_iter().find(|v| !allowed.contains(v)) {
                let reason = format!("`${v}` is not a variable of the en message");
                return Err(unsupported(locale.code, &m.id, &reason));
            }
        }
        let numeric = numeric_vars(locale.source);
        out.push(Lowered {
            locale,
            messages: filled,
            numeric,
        });
    }
    Ok(out)
}

fn unsupported(code: &str, id: &str, reason: &str) -> ExportError {
    ExportError::Unsupported {
        code: code.to_owned(),
        id: id.to_owned(),
        reason: reason.to_owned(),
    }
}

/// Positional argument order per message id: variables in order of first
/// appearance in the `en` message. Translations reuse it, so `{0}`/`%1$@`
/// mean the same argument in every locale even when a translation reorders.
fn arg_order(en: &[Message]) -> HashMap<String, Vec<String>> {
    en.iter().map(|m| (m.id.clone(), vars(&m.shape))).collect()
}

fn vars(shape: &Shape) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut push = |pieces: &[Piece]| {
        for p in pieces {
            if let Piece::Var(v) = p
                && !seen.contains(v)
            {
                seen.push(v.clone());
            }
        }
    };
    match shape {
        Shape::Simple(p) => push(p),
        Shape::Plural {
            selector,
            prefix,
            variants,
            suffix,
        } => {
            push(prefix);
            push(&[Piece::Var(selector.clone())]);
            for (_, v) in variants {
                push(v);
            }
            push(suffix);
        }
    }
    seen
}

/// Variables the source uses as numbers: select selectors and `NUMBER()`
/// arguments. They become `%lld` on Apple; every other variable is `%@`.
fn numeric_vars(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    let bytes = source.as_bytes();
    while let Some(pos) = source[i..].find('$') {
        let start = i + pos + 1;
        let end = source[start..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .map_or(source.len(), |e| start + e);
        let name = &source[start..end];
        let rest = source[end..].trim_start();
        let before = source[..i + pos].trim_end();
        let is_selector = rest.starts_with("->");
        let in_number = before.ends_with("NUMBER(") || before.ends_with("NUMBER( ");
        if (is_selector || in_number) && !name.is_empty() && !out.iter().any(|n| n == name) {
            out.push(name.to_owned());
        }
        i = end.max(start);
        if i >= bytes.len() {
            break;
        }
    }
    out
}

/// Parses one `.ftl` source into export messages.
fn lower_source(code: &str, source: &str) -> Result<Vec<Message>, ExportError> {
    let resource =
        fluent_syntax::parser::parse(source).map_err(|(_, errors)| ExportError::Syntax {
            code: code.to_owned(),
            detail: format!("{:?}", errors.first()),
        })?;
    let mut terms = HashMap::new();
    let mut messages = HashMap::new();
    for entry in &resource.body {
        match entry {
            ast::Entry::Term(t) => {
                terms.insert(t.id.name, &t.value);
            }
            ast::Entry::Message(m) => {
                if let Some(value) = &m.value {
                    messages.insert(m.id.name, value);
                }
            }
            _ => {}
        }
    }
    let cx = Cx {
        code,
        terms: &terms,
        messages: &messages,
    };
    let mut out = Vec::new();
    for entry in &resource.body {
        let ast::Entry::Message(m) = entry else {
            continue;
        };
        // Attributes (`.tooltip = …`) have no counterpart here yet; a message
        // with only attributes is skipped rather than exported empty.
        let Some(value) = &m.value else { continue };
        out.push(Message {
            id: m.id.name.to_owned(),
            shape: cx.shape(m.id.name, value)?,
            comment: m.comment.as_ref().map(|c| c.content.join(" ")),
            fallback: false,
        });
    }
    Ok(out)
}

struct Cx<'a> {
    code: &'a str,
    terms: &'a HashMap<&'a str, &'a ast::Pattern<&'a str>>,
    messages: &'a HashMap<&'a str, &'a ast::Pattern<&'a str>>,
}

impl Cx<'_> {
    fn err(&self, id: &str, reason: &str) -> ExportError {
        unsupported(self.code, id, reason)
    }

    fn shape(&self, id: &str, pattern: &ast::Pattern<&str>) -> Result<Shape, ExportError> {
        let mut prefix = Vec::new();
        let mut select = None;
        let mut suffix = Vec::new();
        for element in &pattern.elements {
            let target = if select.is_some() {
                &mut suffix
            } else {
                &mut prefix
            };
            match element {
                ast::PatternElement::TextElement { value } => push_text(target, value),
                ast::PatternElement::Placeable {
                    expression: ast::Expression::Select { selector, variants },
                } => {
                    if select.is_some() {
                        return Err(self.err(id, "more than one select in a message"));
                    }
                    select = Some(self.select(id, selector, variants)?);
                }
                ast::PatternElement::Placeable {
                    expression: ast::Expression::Inline(inline),
                } => self.inline(id, inline, target, 0)?,
            }
        }
        Ok(match select {
            None => Shape::Simple(prefix),
            Some((selector, variants)) => Shape::Plural {
                selector,
                prefix,
                variants,
                suffix,
            },
        })
    }

    #[allow(clippy::type_complexity)]
    fn select(
        &self,
        id: &str,
        selector: &ast::InlineExpression<&str>,
        variants: &[ast::Variant<&str>],
    ) -> Result<(String, Vec<(String, Vec<Piece>)>), ExportError> {
        let selector = variable_of(selector)
            .ok_or_else(|| self.err(id, "a select must be on a variable or NUMBER($var)"))?;
        let mut out = Vec::new();
        for variant in variants {
            let category = match &variant.key {
                ast::VariantKey::Identifier { name } if CATEGORIES.contains(name) => {
                    (*name).to_owned()
                }
                ast::VariantKey::NumberLiteral { value: "0" } => "zero".to_owned(),
                ast::VariantKey::Identifier { name }
                | ast::VariantKey::NumberLiteral { value: name } => {
                    let reason = format!("variant `[{name}]` is not a CLDR plural category");
                    return Err(self.err(id, &reason));
                }
            };
            if variant.default && category != "other" {
                return Err(self.err(id, "the default variant must be *[other]"));
            }
            let mut pieces = Vec::new();
            for element in &variant.value.elements {
                match element {
                    ast::PatternElement::TextElement { value } => push_text(&mut pieces, value),
                    ast::PatternElement::Placeable {
                        expression: ast::Expression::Inline(inline),
                    } => self.inline(id, inline, &mut pieces, 0)?,
                    ast::PatternElement::Placeable { .. } => {
                        return Err(self.err(id, "a select nested in a variant"));
                    }
                }
            }
            out.push((category, pieces));
        }
        Ok((selector.to_owned(), out))
    }

    /// Inlines literals, terms and message references (their values are
    /// fixed text in every format); keeps variables as placeholders.
    fn inline(
        &self,
        id: &str,
        expr: &ast::InlineExpression<&str>,
        out: &mut Vec<Piece>,
        depth: usize,
    ) -> Result<(), ExportError> {
        if depth > 8 {
            return Err(self.err(id, "references nest too deep (a cycle?)"));
        }
        match expr {
            ast::InlineExpression::StringLiteral { value }
            | ast::InlineExpression::NumberLiteral { value } => push_text(out, value),
            ast::InlineExpression::VariableReference { id: var } => {
                out.push(Piece::Var(var.name.to_owned()));
            }
            ast::InlineExpression::FunctionReference { .. } => match variable_of(expr) {
                Some(var) => out.push(Piece::Var(var.to_owned())),
                None => return Err(self.err(id, "only NUMBER($var) is supported")),
            },
            ast::InlineExpression::TermReference {
                id: term,
                attribute: None,
                arguments: None,
            } => {
                let pattern = self
                    .terms
                    .get(term.name)
                    .ok_or_else(|| self.err(id, &format!("unknown term -{}", term.name)))?;
                self.inline_pattern(id, pattern, out, depth)?;
            }
            ast::InlineExpression::MessageReference {
                id: other,
                attribute: None,
            } => {
                let pattern = self
                    .messages
                    .get(other.name)
                    .ok_or_else(|| self.err(id, &format!("unknown message {}", other.name)))?;
                self.inline_pattern(id, pattern, out, depth)?;
            }
            ast::InlineExpression::Placeable { expression } => match &**expression {
                ast::Expression::Inline(inner) => self.inline(id, inner, out, depth + 1)?,
                ast::Expression::Select { .. } => {
                    return Err(self.err(id, "a select nested in a placeable"));
                }
            },
            _ => return Err(self.err(id, "term arguments and attributes are not supported")),
        }
        Ok(())
    }

    fn inline_pattern(
        &self,
        id: &str,
        pattern: &ast::Pattern<&str>,
        out: &mut Vec<Piece>,
        depth: usize,
    ) -> Result<(), ExportError> {
        for element in &pattern.elements {
            match element {
                ast::PatternElement::TextElement { value } => push_text(out, value),
                ast::PatternElement::Placeable {
                    expression: ast::Expression::Inline(inner),
                } => self.inline(id, inner, out, depth + 1)?,
                ast::PatternElement::Placeable { .. } => {
                    return Err(self.err(id, "a referenced term or message has a select"));
                }
            }
        }
        Ok(())
    }
}

fn variable_of<'s>(expr: &ast::InlineExpression<&'s str>) -> Option<&'s str> {
    match expr {
        ast::InlineExpression::VariableReference { id } => Some(id.name),
        ast::InlineExpression::FunctionReference { id, arguments }
            if id.name == "NUMBER" && arguments.named.is_empty() =>
        {
            match arguments.positional.as_slice() {
                [ast::InlineExpression::VariableReference { id }] => Some(id.name),
                _ => None,
            }
        }
        _ => None,
    }
}

fn push_text(out: &mut Vec<Piece>, text: &str) {
    match out.last_mut() {
        Some(Piece::Text(t)) => t.push_str(text),
        _ => out.push(Piece::Text(text.to_owned())),
    }
}

// ---- Apple ------------------------------------------------------------------

/// Apple format text: `%` doubled, variables as `%@` (strings) or `%lld`
/// (numbers), positional (`%2$@`) once a message has more than one argument.
fn apple(pieces: &[Piece], order: &[String], numeric: &[String]) -> String {
    let mut s = String::new();
    for p in pieces {
        match p {
            Piece::Text(t) => s.push_str(&t.replace('%', "%%")),
            Piece::Var(v) => s.push_str(&apple_spec(v, order, numeric, "")),
        }
    }
    s
}

fn apple_spec(var: &str, order: &[String], numeric: &[String], infix: &str) -> String {
    let kind = if numeric.iter().any(|n| n == var) {
        "lld"
    } else {
        "@"
    };
    let kind = if infix.is_empty() { kind } else { "" };
    match order.iter().position(|v| v == var) {
        Some(i) if order.len() > 1 => format!("%{}${infix}{kind}", i + 1),
        _ => format!("%{infix}{kind}"),
    }
}

/// A plist-safe name for `%#@name@`: Fluent ids may contain `-`.
fn apple_var_name(var: &str) -> String {
    var.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn strings_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn render_strings(
    messages: &[Message],
    order: &HashMap<String, Vec<String>>,
    numeric: &[String],
) -> String {
    let mut s = String::from(
        "/* Generated by ftl-export (cox-i18n) from the Fluent sources. Do not edit. */\n",
    );
    for m in messages {
        let Shape::Simple(pieces) = &m.shape else {
            continue;
        };
        let args = order.get(&m.id).map(Vec::as_slice).unwrap_or_default();
        let note = comment_text(m);
        if !note.is_empty() {
            let _ = write!(s, "\n/* {} */", note.replace("*/", "* /"));
        }
        let _ = write!(
            s,
            "\n\"{}\" = \"{}\";\n",
            strings_escape(&m.id),
            strings_escape(&apple(pieces, args, numeric))
        );
    }
    s
}

fn comment_text(m: &Message) -> String {
    let mut parts = Vec::new();
    if let Some(c) = &m.comment {
        parts.push(c.clone());
    }
    if m.fallback {
        parts.push(format!("fallback: {DEFAULT_LOCALE}"));
    }
    parts.join(" | ")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_stringsdict(
    messages: &[Message],
    order: &HashMap<String, Vec<String>>,
    numeric: &[String],
) -> String {
    let mut s = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
        "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
        "<!-- Generated by ftl-export (cox-i18n) from the Fluent sources. Do not edit. -->\n",
        "<plist version=\"1.0\">\n<dict>\n",
    ));
    for m in messages {
        let Shape::Plural {
            selector,
            prefix,
            variants,
            suffix,
        } = &m.shape
        else {
            continue;
        };
        let args = order.get(&m.id).map(Vec::as_slice).unwrap_or_default();
        // The selector is numeric by construction, whatever the source said.
        let mut numeric = numeric.to_vec();
        numeric.push(selector.clone());
        let name = apple_var_name(selector);
        let format_key = format!(
            "{}{}{}",
            apple(prefix, args, &numeric),
            apple_spec(selector, args, &numeric, &format!("#@{name}@")),
            apple(suffix, args, &numeric),
        );
        let note = comment_text(m);
        if !note.is_empty() {
            let _ = writeln!(s, "  <!-- {} -->", xml_escape(&note).replace("--", "- -"));
        }
        let _ = writeln!(s, "  <key>{}</key>\n  <dict>", xml_escape(&m.id));
        let _ = writeln!(
            s,
            "    <key>NSStringLocalizedFormatKey</key>\n    <string>{}</string>",
            xml_escape(&format_key)
        );
        let _ = writeln!(s, "    <key>{name}</key>\n    <dict>");
        s.push_str("      <key>NSStringFormatSpecTypeKey</key>\n");
        s.push_str("      <string>NSStringPluralRuleType</string>\n");
        s.push_str("      <key>NSStringFormatValueTypeKey</key>\n      <string>lld</string>\n");
        for (category, pieces) in variants {
            let _ = writeln!(
                s,
                "      <key>{category}</key>\n      <string>{}</string>",
                xml_escape(&apple(pieces, args, &numeric))
            );
        }
        s.push_str("    </dict>\n  </dict>\n");
    }
    s.push_str("</dict>\n</plist>\n");
    s
}

// ---- Windows ----------------------------------------------------------------

/// .NET composite format text: `{`/`}` doubled, variables as `{0}`, `{1}` in
/// the `en` argument order.
fn dotnet(pieces: &[Piece], order: &[String]) -> String {
    let mut s = String::new();
    for p in pieces {
        match p {
            Piece::Text(t) => s.push_str(&t.replace('{', "{{").replace('}', "}}")),
            Piece::Var(v) => {
                let i = order.iter().position(|o| o == v).unwrap_or_default();
                let _ = write!(s, "{{{i}}}");
            }
        }
    }
    s
}

const RESW_HEADER: &str = concat!(
    "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n",
    "<!-- Generated by ftl-export (cox-i18n) from the Fluent sources. Do not edit. -->\n",
    "<root>\n",
    "  <resheader name=\"resmimetype\">\n    <value>text/microsoft-resx</value>\n  </resheader>\n",
    "  <resheader name=\"version\">\n    <value>2.0</value>\n  </resheader>\n",
    "  <resheader name=\"reader\">\n",
    "    <value>System.Resources.ResXResourceReader, System.Windows.Forms, ",
    "Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089</value>\n",
    "  </resheader>\n",
    "  <resheader name=\"writer\">\n",
    "    <value>System.Resources.ResXResourceWriter, System.Windows.Forms, ",
    "Version=4.0.0.0, Culture=neutral, PublicKeyToken=b77a5c561934e089</value>\n",
    "  </resheader>\n",
);

/// `.resw` has no plural rules: a select becomes one entry per category,
/// `<id>_<category>`, and the app picks the category (`docs/i18n.md`).
fn render_resw(messages: &[Message], order: &HashMap<String, Vec<String>>) -> String {
    let mut s = String::from(RESW_HEADER);
    let mut entry = |name: &str, value: &str, note: &str| {
        let _ = writeln!(
            s,
            "  <data name=\"{}\" xml:space=\"preserve\">\n    <value>{}</value>",
            xml_escape(name),
            xml_escape(value)
        );
        if !note.is_empty() {
            let _ = writeln!(s, "    <comment>{}</comment>", xml_escape(note));
        }
        s.push_str("  </data>\n");
    };
    for m in messages {
        let args = order.get(&m.id).map(Vec::as_slice).unwrap_or_default();
        let note = comment_text(m);
        match &m.shape {
            Shape::Simple(pieces) => entry(&m.id, &dotnet(pieces, args), &note),
            Shape::Plural {
                prefix,
                variants,
                suffix,
                ..
            } => {
                for (category, pieces) in variants {
                    let value = format!(
                        "{}{}{}",
                        dotnet(prefix, args),
                        dotnet(pieces, args),
                        dotnet(suffix, args)
                    );
                    entry(&format!("{}_{category}", m.id), &value, &note);
                }
            }
        }
    }
    s.push_str("</root>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lowered(code: &str) -> Lowered {
        lower_all()
            .expect("lowers")
            .into_iter()
            .find(|l| l.locale.code == code)
            .expect("locale exists")
    }

    fn en_order() -> HashMap<String, Vec<String>> {
        arg_order(&lowered("en").messages)
    }

    #[test]
    fn translations_are_filled_from_english_per_message() {
        for code in ["ru", "uk"] {
            let l = lowered(code);
            let m = l
                .messages
                .iter()
                .find(|m| m.id == "send-feedback")
                .expect("filled");
            assert!(m.fallback);
            assert_eq!(
                m.shape,
                Shape::Simple(vec![Piece::Text("Send feedback".into())])
            );
            let own = l
                .messages
                .iter()
                .find(|m| m.id == "settings-title")
                .expect("own");
            assert!(!own.fallback);
        }
    }

    #[test]
    fn apple_strings_inline_terms_and_map_variables() {
        let l = lowered("uk");
        let s = render_strings(&l.messages, &en_order(), &l.numeric);
        assert!(s.contains("\"quit-app\" = \"Вийти з Cox\";"), "{s}");
        assert!(
            s.contains("\"welcome-user\" = \"Ласкаво просимо до Cox, %@!\";"),
            "{s}"
        );
        assert!(
            s.contains("fallback: en */\n\"send-feedback\" = \"Send feedback\";"),
            "{s}"
        );
        assert!(
            !s.contains("session-count"),
            "plurals belong in the stringsdict"
        );
    }

    #[test]
    fn stringsdict_uses_positional_arguments_for_two_variables() {
        let l = lowered("ru");
        let s = render_stringsdict(&l.messages, &en_order(), &l.numeric);
        assert!(
            s.contains("<string>%1$@ изменяет %2$#@count@</string>"),
            "{s}"
        );
        assert!(
            s.contains("<key>many</key>\n      <string>%2$lld файлов</string>"),
            "{s}"
        );
        assert!(s.contains("<string>%#@count@</string>"), "{s}");
        assert!(
            s.contains("<key>few</key>\n      <string>%lld сессии</string>"),
            "{s}"
        );
    }

    #[test]
    fn resw_splits_plurals_into_category_keys() {
        let l = lowered("uk");
        let s = render_resw(&l.messages, &en_order());
        for category in ["one", "few", "many", "other"] {
            assert!(
                s.contains(&format!("name=\"session-count_{category}\"")),
                "{s}"
            );
        }
        assert!(s.contains("<value>{0} змінює {1} файлів</value>"), "{s}");
        assert!(
            s.contains("<value>Ласкаво просимо до Cox, {0}!</value>"),
            "{s}"
        );
    }

    #[test]
    fn xml_comments_hold_no_double_hyphen() {
        // `--` inside `<!-- -->` makes the file malformed XML (xmllint rejects it).
        let l = lowered("ru");
        for xml in [
            render_resw(&l.messages, &en_order()),
            render_stringsdict(&l.messages, &en_order(), &l.numeric),
        ] {
            for comment in xml.split("<!--").skip(1) {
                let body = comment.split("-->").next().unwrap_or_default();
                assert!(!body.contains("--"), "{body}");
            }
        }
    }

    #[test]
    fn literal_braces_and_percent_are_escaped() {
        let pieces = vec![Piece::Text("100% {x}".into()), Piece::Var("n".into())];
        let order = vec!["n".to_owned()];
        assert_eq!(apple(&pieces, &order, &["n".to_owned()]), "100%% {x}%lld");
        assert_eq!(dotnet(&pieces, &order), "100% {{x}}{0}");
    }

    #[test]
    fn non_category_variant_keys_are_rejected() {
        let src = "m = { $n ->\n    [7] seven\n   *[other] many\n}\n";
        let err = lower_source("en", src).expect_err("rejects [7]");
        assert!(
            err.to_string().contains("not a CLDR plural category"),
            "{err}"
        );
    }
}
