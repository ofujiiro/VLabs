// SPDX-License-Identifier: GPL-3.0-or-later

//! `cargo xtask guide`: the user guide (`docs/user-guide.md`) as one
//! self-contained HTML page, its pictures inside it, to ship next to the
//! program (decision 0022).
//!
//! The guide is written in a small, plain part of Markdown, which this
//! converts with the standard library only: headings, paragraphs, lists,
//! tables, quotes (shown as tips), code, bold, italics, links and
//! pictures.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// Converts the guide at `source` into a page at `out`. Pictures are read
/// relative to the guide's folder.
pub fn render_file(source: &Path, out: &Path, title: &str) -> Result<(), String> {
    let text = fs::read_to_string(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let folder = source.parent().unwrap_or(Path::new("."));
    let html = render(&text, title, &|path| fs::read(folder.join(path)).ok());
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(out, html).map_err(|e| format!("{}: {e}", out.display()))
}

/// The page for a guide's text; `picture` gives a picture's bytes by the
/// path the guide names.
#[allow(
    clippy::too_many_lines,
    reason = "one pass over the blocks of the text"
)]
pub fn render(text: &str, title: &str, picture: &dyn Fn(&str) -> Option<Vec<u8>>) -> String {
    let mut body = String::new();
    let mut contents = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        if let Some(fence) = trimmed.strip_prefix("```") {
            let _ = fence;
            let mut code = String::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push_str(&escape(lines[i]));
                code.push('\n');
                i += 1;
            }
            i += 1;
            let _ = writeln!(body, "<pre><code>{code}</code></pre>");
            continue;
        }
        if let Some((level, heading)) = heading(trimmed) {
            let id = slug(heading);
            if level == 2 {
                contents.push((id.clone(), inline(heading, picture)));
            }
            let _ = writeln!(
                body,
                "<h{level} id=\"{id}\">{}</h{level}>",
                inline(heading, picture)
            );
            i += 1;
            continue;
        }
        if trimmed == "---" {
            body.push_str("<hr>\n");
            i += 1;
            continue;
        }
        if trimmed.starts_with('|') {
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('|') {
                rows.push(lines[i].trim());
                i += 1;
            }
            body.push_str(&table(&rows, picture));
            continue;
        }
        if trimmed.starts_with('>') {
            let mut quote = String::new();
            while i < lines.len() && lines[i].trim().starts_with('>') {
                quote.push_str(lines[i].trim().trim_start_matches('>').trim());
                quote.push(' ');
                i += 1;
            }
            let _ = writeln!(
                body,
                "<aside class=\"tip\">{}</aside>",
                inline(quote.trim(), picture)
            );
            continue;
        }
        if let Some(kind) = list_kind(trimmed) {
            let tag = if kind { "ol" } else { "ul" };
            let _ = writeln!(body, "<{tag}>");
            while i < lines.len() && list_kind(lines[i].trim()) == Some(kind) {
                let mut item = list_text(lines[i].trim()).to_owned();
                i += 1;
                // Lines indented under an item continue it.
                while i < lines.len()
                    && !lines[i].trim().is_empty()
                    && lines[i].starts_with("  ")
                    && list_kind(lines[i].trim()).is_none()
                {
                    item.push(' ');
                    item.push_str(lines[i].trim());
                    i += 1;
                }
                let _ = writeln!(body, "<li>{}</li>", inline(&item, picture));
            }
            let _ = writeln!(body, "</{tag}>");
            continue;
        }
        // A paragraph: lines until a blank line or another block.
        let mut paragraph = String::new();
        while i < lines.len() {
            let t = lines[i].trim();
            if t.is_empty()
                || heading(t).is_some()
                || t.starts_with("```")
                || t.starts_with('|')
                || t.starts_with('>')
                || (list_kind(t).is_some() && !paragraph.is_empty())
            {
                break;
            }
            paragraph.push_str(t);
            paragraph.push(' ');
            i += 1;
        }
        let _ = writeln!(body, "<p>{}</p>", inline(paragraph.trim(), picture));
    }
    let mut toc = String::new();
    if contents.len() > 2 {
        toc.push_str("<nav><strong>Contents</strong><ol>\n");
        for (id, heading) in &contents {
            let _ = writeln!(toc, "<li><a href=\"#{id}\">{heading}</a></li>");
        }
        toc.push_str("</ol></nav>\n");
    }
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n{toc}{body}</main>\n</body>\n</html>\n",
        escape(title)
    )
}

const STYLE: &str = "
:root { color-scheme: light dark; --text: #1d2125; --weak: #5a6570; --accent: #1e8f84;
  --surface: #f4f6f7; --line: #d7dde1; --background: #ffffff; }
@media (prefers-color-scheme: dark) { :root { --text: #e6eaed; --weak: #9aa6b0;
  --accent: #48c7ba; --surface: #1d2126; --line: #333a41; --background: #15181b; } }
body { margin: 0; background: var(--background); color: var(--text);
  font: 17px/1.6 system-ui, -apple-system, 'Segoe UI', sans-serif; }
main { max-width: 46rem; margin: 0 auto; padding: 2rem 1.25rem 4rem; }
h1 { font-size: 2.1rem; margin-top: 0; } h2 { margin-top: 2.6rem; border-top: 1px solid var(--line);
  padding-top: 1.6rem; } h3 { margin-top: 1.8rem; }
a { color: var(--accent); } img { max-width: 100%; border-radius: 8px; border: 1px solid var(--line); }
code { background: var(--surface); padding: 0.1em 0.35em; border-radius: 4px; font-size: 0.92em; }
pre { background: var(--surface); padding: 0.9rem 1rem; border-radius: 8px; overflow-x: auto; }
pre code { background: none; padding: 0; }
table { border-collapse: collapse; width: 100%; margin: 1rem 0; font-size: 0.95rem; }
th, td { text-align: left; padding: 0.4rem 0.6rem; border-bottom: 1px solid var(--line); vertical-align: top; }
kbd { border: 1px solid var(--line); border-bottom-width: 2px; border-radius: 4px; padding: 0 0.3em;
  font: 0.9em ui-monospace, monospace; background: var(--surface); }
.tip { background: var(--surface); border-left: 4px solid var(--accent); padding: 0.7rem 1rem;
  border-radius: 0 8px 8px 0; margin: 1rem 0; }
nav { background: var(--surface); border-radius: 8px; padding: 0.8rem 1.2rem; margin: 1.5rem 0; }
nav ol { margin: 0.4rem 0 0; padding-left: 1.3rem; } hr { border: 0; border-top: 1px solid var(--line); }
";

fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|&c| c == '#').count();
    ((1..=4).contains(&level) && line[level..].starts_with(' '))
        .then(|| (level, line[level..].trim()))
}

/// `Some(true)` for a numbered item, `Some(false)` for a bullet.
fn list_kind(line: &str) -> Option<bool> {
    if line.starts_with("- ") || line.starts_with("* ") {
        return Some(false);
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && line[digits..].starts_with(". ")).then_some(true)
}

fn list_text(line: &str) -> &str {
    match list_kind(line) {
        Some(false) => &line[2..],
        Some(true) => {
            let digits = line.chars().take_while(char::is_ascii_digit).count();
            &line[digits + 2..]
        }
        None => line,
    }
}

fn slug(text: &str) -> String {
    let mut slug = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}

fn table(rows: &[&str], picture: &dyn Fn(&str) -> Option<Vec<u8>>) -> String {
    let cells = |row: &str| -> Vec<String> {
        row.trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_owned())
            .collect()
    };
    let mut out = String::from("<table>\n");
    for (index, row) in rows.iter().enumerate() {
        // The line of dashes under the header.
        if row.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
            continue;
        }
        let tag = if index == 0 { "th" } else { "td" };
        out.push_str("<tr>");
        for cell in cells(row) {
            let _ = write!(out, "<{tag}>{}</{tag}>", inline(&cell, picture));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Bold, italics, code, keys, links and pictures inside a line.
fn inline(text: &str, picture: &dyn Fn(&str) -> Option<Vec<u8>>) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let rest = |from: usize| chars[from..].iter().collect::<String>();
    while i < chars.len() {
        let c = chars[i];
        if c == '`'
            && let Some(end) = chars[i + 1..].iter().position(|&x| x == '`')
        {
            let code: String = chars[i + 1..i + 1 + end].iter().collect();
            let _ = write!(out, "<code>{}</code>", escape(&code));
            i += end + 2;
            continue;
        }
        if c == '!'
            && chars.get(i + 1) == Some(&'[')
            && let Some((alt, url, used)) = link(&rest(i + 1))
        {
            let source = picture(&url).map_or_else(
                || escape(&url),
                |bytes| format!("data:{};base64,{}", media_type(&url), base64(&bytes)),
            );
            let _ = write!(out, "<img src=\"{source}\" alt=\"{}\">", escape(&alt));
            i += 1 + used;
            continue;
        }
        if c == '['
            && let Some((label, url, used)) = link(&rest(i))
        {
            let _ = write!(
                out,
                "<a href=\"{}\">{}</a>",
                escape(&url),
                inline(&label, picture)
            );
            i += used;
            continue;
        }
        if c == '*'
            && chars.get(i + 1) == Some(&'*')
            && let Some(end) = rest(i + 2).find("**")
        {
            let inner: String = rest(i + 2).chars().take(end).collect();
            let _ = write!(out, "<strong>{}</strong>", inline(&inner, picture));
            i += 2 + inner.chars().count() + 2;
            continue;
        }
        if c == '*' || c == '_' {
            let start_ok = i == 0 || !chars[i - 1].is_alphanumeric();
            if start_ok
                && let Some(end) = chars[i + 1..].iter().position(|&x| x == c)
                && end > 0
            {
                let inner: String = chars[i + 1..i + 1 + end].iter().collect();
                let _ = write!(out, "<em>{}</em>", inline(&inner, picture));
                i += end + 2;
                continue;
            }
        }
        if c == '<'
            && rest(i).starts_with("<kbd>")
            && let Some(end) = rest(i).find("</kbd>")
        {
            let key: String = rest(i + 5).chars().take(end - 5).collect();
            let _ = write!(out, "<kbd>{}</kbd>", escape(&key));
            i += end + 6;
            continue;
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
        i += 1;
    }
    out
}

/// `[label](url)` at the start of `text`: the label, the address, and how
/// many characters it took.
fn link(text: &str) -> Option<(String, String, usize)> {
    let close = text.find("](")?;
    let label = text.get(1..close)?;
    let after = &text[close + 2..];
    let end = after.find(')')?;
    let url = &after[..end];
    let used = label.chars().count() + url.chars().count() + 4;
    Some((label.to_owned(), url.to_owned(), used))
}

fn media_type(path: &str) -> &'static str {
    let ending = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase());
    match ending.as_deref() {
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        _ => "image/jpeg",
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guides_markdown_becomes_html() {
        let page = render(
            "# Guide\n\nSome **bold** and *soft* text with `code` and <kbd>Ctrl+S</kbd>.\n\n\
             ## First steps\n\n1. One\n2. Two\n   still two\n\n- a [link](https://example.org)\n\n\
             > A tip.\n\n| Key | Does |\n| --- | --- |\n| Space | Plays |\n\n![A picture](shot.png)\n\n\
             ## More\n\n## Last\n",
            "Guide",
            &|path| (path == "shot.png").then(|| vec![1, 2, 3]),
        );
        assert!(page.contains("<h1 id=\"guide\">Guide</h1>"));
        assert!(page.contains("<strong>bold</strong>") && page.contains("<em>soft</em>"));
        assert!(page.contains("<code>code</code>") && page.contains("<kbd>Ctrl+S</kbd>"));
        assert!(page.contains("<li>Two still two</li>"));
        assert!(page.contains("<a href=\"https://example.org\">link</a>"));
        assert!(page.contains("<aside class=\"tip\">A tip.</aside>"));
        assert!(page.contains("<th>Key</th>") && page.contains("<td>Plays</td>"));
        assert!(page.contains("data:image/png;base64,AQID"));
        assert!(
            page.contains("<a href=\"#first-steps\">First steps</a>"),
            "contents"
        );
    }

    #[test]
    fn text_is_escaped() {
        let page = render("A <b> & \"c\"", "T", &|_| None);
        assert!(page.contains("A &lt;b&gt; &amp; \"c\""));
    }
}
