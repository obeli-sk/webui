use once_cell::sync::Lazy;
use std::cmp::min;
use std::rc::Rc;
use syntect::html::{ClassStyle, line_tokens_to_classed_spans};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};
use syntect::util::LinesWithEndings;
use yew::{prelude::*, virtual_dom::VNode};

pub const DEFAULT_CONTEXT_LINES: usize = 3;
const DEFAULT_THEME: &str = "base16-ocean.dark"; // NB: Sync with build.rs

// bat's curated syntaxes, a superset of the defaults that adds TOML.
pub static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(two_face::syntax::extra_newlines);

pub fn highlight_code_line_by_line(
    source: &str,
    language_ext: Option<&str>,
) -> Vec<(Html, usize /* line number */)> {
    let start = web_sys::window().unwrap().performance().unwrap().now();
    let output_lines = highlight_lines(source, language_ext)
        .into_iter()
        .enumerate()
        .map(|(idx, line)| {
            let line = match line {
                Ok(line) => VNode::from_html_unchecked(line.into()),
                Err(line) => Html::from(line), // Display as plain text
            };
            (line, idx + 1) // Store with 1-based line number
        })
        .collect();
    let end = web_sys::window().unwrap().performance().unwrap().now();
    log::trace!("Highlighted in {}ms", end - start);
    output_lines
}

/// Highlight each line into self-contained HTML, keeping parser state across lines.
fn highlight_lines<'a>(
    source: &'a str,
    language_ext: Option<&str>,
) -> Vec<Result<String, &'a str>> {
    const DEFAULT_LANGUAGE: &str = "txt";
    let language_ext = language_ext.unwrap_or(DEFAULT_LANGUAGE);
    let syntax = SYNTAX_SET
        .find_syntax_by_extension(language_ext)
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());
    let mut parse_state = ParseState::new(syntax);
    let mut scope_stack = ScopeStack::new();
    LinesWithEndings::from(source)
        .map(|line| {
            // Reopen the scopes left open by previous lines.
            let mut html: String = scope_stack
                .as_slice()
                .iter()
                .map(|scope| {
                    format!(
                        "<span class=\"{}\">",
                        scope.build_string().replace('.', " ")
                    )
                })
                .collect();
            let ops = parse_state
                .parse_line(line, &SYNTAX_SET)
                .map_err(|_| line)?;
            let (spans, _) =
                line_tokens_to_classed_spans(line, &ops, ClassStyle::Spaced, &mut scope_stack)
                    .map_err(|_| line)?;
            html.push_str(&spans);
            html.push_str(&"</span>".repeat(scope_stack.len()));
            Ok(html)
        })
        .collect()
}

#[derive(Properties, PartialEq)]
pub struct CodeBlockProps {
    pub source: Rc<[(Html, usize)]>,
    pub focus_line: Option<usize>,
    /// How many lines above the focus line to show (controlled by parent).
    pub lines_above: usize,
    /// How many lines below the focus line to show (controlled by parent).
    pub lines_below: usize,
    /// Called with `(new_lines_above, new_lines_below)` when the user expands.
    pub on_expand: Callback<(usize, usize)>,
}

enum ExpandDirection {
    Above,
    Below,
    All,
}

#[component(SyntectCodeBlock)]
pub fn code_block(props: &CodeBlockProps) -> Html {
    let total_lines = props.source.len();
    let lines_above = props.lines_above;
    let lines_below = props.lines_below;

    // Derive the visible range from focus + offsets each render.
    let (visible_start_idx, visible_end_idx) = if let Some(focus_line) = props.focus_line {
        let focus_idx = focus_line.saturating_sub(1);
        let start = focus_idx.saturating_sub(lines_above);
        let end = min(total_lines, focus_idx + lines_below + 1);
        (start, end)
    } else {
        (0, total_lines)
    };

    let show_expand_above = visible_start_idx > 0;
    let show_expand_below = visible_end_idx < total_lines;

    let handle_expand = {
        let on_expand = props.on_expand.clone();
        Callback::from(move |direction: ExpandDirection| {
            let (new_above, new_below) = match direction {
                ExpandDirection::Above => (lines_above + DEFAULT_CONTEXT_LINES * 2, lines_below),
                ExpandDirection::Below => (lines_above, lines_below + DEFAULT_CONTEXT_LINES * 2),
                ExpandDirection::All => (total_lines, total_lines),
            };
            on_expand.emit((new_above, new_below));
        })
    };

    let lines_to_render = props
        .source
        .get(visible_start_idx..visible_end_idx)
        .unwrap_or_default();

    html! {
        <div class={classes!("code-block-container", format!("theme-{DEFAULT_THEME}"))}>
            { if show_expand_above {
                html!{
                    <button class="expand-button expand-above" onclick={handle_expand.reform(move |_| ExpandDirection::Above)}>
                        { format!("Expand {} lines above", min(visible_start_idx, DEFAULT_CONTEXT_LINES*2)) }
                    </button>
                }
            } else {
                html!{}
            } }

            { if show_expand_above || show_expand_below {
                 html!{
                    <button class="expand-button expand-all" onclick={handle_expand.reform(move |_| ExpandDirection::All)}>
                        { "Expand All" }
                    </button>
                 }
            } else {
                html!{}
            } }

            // Each source line occupies exactly one <tr>, keeping the line number
            // and its code content in the same row.  This prevents any mismatch
            // between the number column and the code column regardless of font
            // size or zoom level.  Horizontal scrolling is handled by the
            // .code-scroll-area wrapper so lines are never wrapped.
            <div class="code-scroll-area">
                <table class="syntect-block">
                    { for lines_to_render.iter().map(|(line_html, line_num)| {
                        let is_focused = props.focus_line == Some(*line_num);
                        let row_class = classes!(is_focused.then_some("line-focused"));
                        html! {
                            <tr class={row_class}>
                                <td class="line-number" aria-hidden="true">{ *line_num }</td>
                                <td class="line-code">{ line_html.clone() }</td>
                            </tr>
                        }
                    }) }
                </table>
            </div>

            { if show_expand_below {
                html!{
                    <button class="expand-button expand-below" onclick={handle_expand.reform(move |_| ExpandDirection::Below)}>
                         { format!("Expand {} lines below", min(total_lines - visible_end_idx, DEFAULT_CONTEXT_LINES*2)) }
                    </button>
                }
            } else {
                 html!{}
            } }
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highlight_toml(source: &str) -> Vec<String> {
        highlight_lines(source, Some("toml"))
            .into_iter()
            .map(|line| line.unwrap())
            .collect()
    }

    #[test]
    fn toml_is_highlighted() {
        let lines = highlight_toml(
            "[[activity_wasm]]\nmax_retries = 5 # retries\nlocation.oci = \"docker.io/x\"\n",
        );
        assert!(lines[0].contains("entity name table toml"));
        assert!(lines[1].contains("entity name tag toml"));
        assert!(lines[1].contains("constant numeric integer toml"));
        assert!(lines[1].contains("comment line number-sign toml"));
        assert!(lines[2].contains("string quoted double basic toml"));
    }

    #[test]
    fn multiline_constructs_keep_parser_state() {
        let lines = highlight_toml("env = [\n  { name = \"A\", optional = true },\n]\n");
        assert!(
            lines.iter().all(|line| !line.contains("invalid")),
            "{lines:#?}"
        );
        assert!(lines[1].starts_with("<span class=\"source toml\">"));
        assert!(lines[1].contains("string quoted double basic toml"));
    }
}
