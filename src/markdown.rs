//! Markdown to Typst markup.
//!
//! The Markdown is untrusted, so nothing in it may reach Typst as code: every piece of text is
//! escaped character by character, code goes through `raw(...)` string literals, and link
//! targets are string literals too. Structure (headings, lists, tables, quotes) is emitted as
//! Typst function calls, which nest without indentation rules. Images become their alt text
//! (the document cannot read files).

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Convert Markdown to a complete Typst document body (no `#set` rules).
pub fn to_typst(markdown: &str) -> String {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut w = Writer::default();
    for event in Parser::new_ext(markdown, opts) {
        w.event(event);
    }
    w.out
}

#[derive(Default)]
struct Writer {
    out: String,
    /// Text of the code block being collected, with its language.
    code: Option<(String, String)>,
    /// Alt text of the image being collected.
    image: Option<String>,
    /// Nesting of table rows: `true` while inside the header row.
    in_table_head: bool,
}

impl Writer {
    fn event(&mut self, event: Event<'_>) {
        if let Some((_, buf)) = self.code.as_mut() {
            match event {
                Event::Text(t) => buf.push_str(&t),
                Event::End(TagEnd::CodeBlock) => {
                    let (lang, text) = self.code.take().unwrap_or_default();
                    let lang = if lang.is_empty() {
                        String::new()
                    } else {
                        format!("lang: {}, ", string(&lang))
                    };
                    self.out.push_str(&format!(
                        "\n#raw(block: true, {lang}{})\n\n",
                        string(text.trim_end_matches('\n'))
                    ));
                }
                _ => {}
            }
            return;
        }
        if let Some(alt) = self.image.as_mut() {
            match event {
                Event::Text(t) | Event::Code(t) => alt.push_str(&t),
                Event::End(TagEnd::Image) => {
                    let alt = self.image.take().unwrap_or_default();
                    let label = if alt.is_empty() {
                        "[image]".to_string()
                    } else {
                        format!("[image: {alt}]")
                    };
                    self.out.push_str(&format!("#emph[{}]", escape(&label)));
                }
                _ => {}
            }
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => self.out.push_str(&escape(&t)),
            Event::Code(t) => self.out.push_str(&format!("#raw({})", string(&t))),
            Event::InlineMath(t) | Event::DisplayMath(t) => {
                self.out.push_str(&format!("#raw({})", string(&t)))
            }
            Event::Html(t) | Event::InlineHtml(t) => self.out.push_str(&escape(&t)),
            Event::SoftBreak => self.out.push(' '),
            Event::HardBreak => self.out.push_str("\\\n"),
            Event::Rule => self.out.push_str("\n#line(length: 100%)\n\n"),
            Event::TaskListMarker(done) => {
                self.out
                    .push_str(if done { "\u{2611} " } else { "\u{2610} " })
            }
            Event::FootnoteReference(t) => self.out.push_str(&escape(&format!("[{t}]"))),
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                let n = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.out.push_str(&format!("\n#heading(level: {n})["));
            }
            Tag::BlockQuote(_) => self.out.push_str("\n#quote(block: true)["),
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                self.code = Some((lang, String::new()));
            }
            Tag::List(Some(start)) => self.out.push_str(&format!("\n#enum(start: {start}, ")),
            Tag::List(None) => self.out.push_str("\n#list("),
            Tag::Item => self.out.push('['),
            Tag::Emphasis => self.out.push_str("#emph["),
            Tag::Strong => self.out.push_str("#strong["),
            Tag::Strikethrough => self.out.push_str("#strike["),
            Tag::Link { dest_url, .. } => {
                self.out.push_str(&format!("#link({})[", string(&dest_url)));
            }
            Tag::Image { .. } => self.image = Some(String::new()),
            Tag::Table(aligns) => {
                self.out
                    .push_str(&format!("\n#table(columns: {}, ", aligns.len().max(1)));
            }
            Tag::TableHead => self.in_table_head = true,
            Tag::TableRow => {}
            Tag::TableCell => {
                self.out
                    .push_str(if self.in_table_head { "[#strong[" } else { "[" });
            }
            Tag::FootnoteDefinition(label) => {
                self.out
                    .push_str(&format!("\n{} ", escape(&format!("[{label}]:"))));
            }
            // Containers without a Typst counterpart: keep their text.
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.out.push_str("\n\n"),
            TagEnd::Heading(_) => self.out.push_str("]\n\n"),
            TagEnd::BlockQuote(_) => self.out.push_str("]\n\n"),
            TagEnd::List(_) => self.out.push_str(")\n\n"),
            TagEnd::Item => {
                // Trailing paragraph breaks inside an item would only add space.
                while self.out.ends_with('\n') {
                    self.out.pop();
                }
                self.out.push_str("], ");
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.out.push(']')
            }
            TagEnd::Table => self.out.push_str(")\n\n"),
            TagEnd::TableHead => self.in_table_head = false,
            TagEnd::TableCell => {
                self.out
                    .push_str(if self.in_table_head { "]], " } else { "], " });
            }
            TagEnd::FootnoteDefinition => self.out.push_str("\n\n"),
            _ => {}
        }
    }
}

/// Escape text so Typst reads every character literally: ASCII punctuation gets a backslash
/// (a backslash before any non-space character is an escape in Typst markup).
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    for c in text.chars() {
        if c.is_ascii_punctuation() {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A Typst string literal.
pub fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_cannot_become_code() {
        let t = to_typst("#import \"x\": * and $math$ and <label> @ref [x]");
        assert!(t.contains("\\#import"), "{t}");
        assert!(t.contains("\\$math\\$"), "{t}");
        assert!(!t.contains(" #import"), "{t}");
    }

    #[test]
    fn structure() {
        let t = to_typst("# Title\n\n- a\n- b\n\n1. one\n\n```rust\nfn main() {}\n```\n");
        assert!(t.contains("#heading(level: 1)[Title]"), "{t}");
        assert!(t.contains("#list([a], [b], )"), "{t}");
        assert!(t.contains("#enum(start: 1, [one], )"), "{t}");
        assert!(
            t.contains("#raw(block: true, lang: \"rust\", \"fn main() {}\")"),
            "{t}"
        );
    }

    #[test]
    fn strings_are_escaped() {
        assert_eq!(string("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }
}
