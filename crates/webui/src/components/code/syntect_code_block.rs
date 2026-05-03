use once_cell::sync::Lazy;
use std::cmp::min;
use std::rc::Rc;
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use yew::{prelude::*, virtual_dom::VNode};

pub const DEFAULT_CONTEXT_LINES: usize = 3;
const DEFAULT_THEME: &str = "base16-ocean.dark"; // NB: Sync with build.rs

pub static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);

pub fn highlight_code_line_by_line(
    source: &str,
    language_ext: Option<&str>,
) -> Vec<(Html, usize /* line number */)> {
    const DEFAULT_LANGUAGE: &str = "txt";
    let language_ext = language_ext.unwrap_or(DEFAULT_LANGUAGE);
    let start = web_sys::window().unwrap().performance().unwrap().now();
    let syntax = SYNTAX_SET
        .find_syntax_by_extension(language_ext)
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    let mut output_lines = Vec::new();
    for (line_num, line) in LinesWithEndings::from(source).enumerate() {
        let line_num = line_num + 1; // Store with 1-based line number
        let mut highlighter =
            ClassedHTMLGenerator::new_with_class_style(syntax, &SYNTAX_SET, ClassStyle::Spaced);
        if let Err(_err) = highlighter.parse_html_for_line_which_includes_newline(line) {
            // Display as plain text
            let line = Html::from(line);
            output_lines.push((line, line_num));
        } else {
            // Note: The generated HTML usually doesn't include the outer <span> or <pre> tags per line.
            // We wrap each line in a span or div later in the component.
            let line = highlighter.finalize();
            let line = VNode::from_html_unchecked(line.into());
            output_lines.push((line, line_num));
        }
    }
    let end = web_sys::window().unwrap().performance().unwrap().now();
    log::trace!("Highlighted in {}ms", end - start);
    output_lines
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
