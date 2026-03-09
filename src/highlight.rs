use std::io::{self, IsTerminal, Write};

use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

/// Prints text to stdout with syntax highlighting when stdout is a TTY.
/// Falls back to plain output otherwise.
/// `theme_name` should be a syntect built-in theme name.
pub fn print_highlighted(text: &str, syntax_name: &str, theme_name: &str) {
    if !io::stdout().is_terminal() {
        print!("{}", text);
        return;
    }

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();

    let syntax = ss
        .find_syntax_by_token(syntax_name)
        .unwrap_or_else(|| ss.find_syntax_plain_text());

    let theme = ts
        .themes
        .get(theme_name)
        .unwrap_or_else(|| &ts.themes["base16-ocean.dark"]);

    let mut h = HighlightLines::new(syntax, theme);
    let mut out = io::stdout().lock();

    for line in LinesWithEndings::from(text) {
        match h.highlight_line(line, &ss) {
            Ok(ranges) => {
                let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
                let _ = write!(out, "{}", escaped);
            }
            Err(_) => {
                let _ = write!(out, "{}", line);
            }
        }
    }
    // Reset terminal colors.
    let _ = write!(out, "\x1b[0m");
}

/// Returns the names of all available built-in themes.
pub fn available_themes() -> Vec<String> {
    let ts = ThemeSet::load_defaults();
    let mut names: Vec<String> = ts.themes.keys().cloned().collect();
    names.sort();
    names
}
