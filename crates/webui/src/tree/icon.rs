//! Icon component using Unicode characters/emoji.

use yew::prelude::*;

/// Icon variants used throughout the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Icon {
    #[default]
    Blank,
    Antenna,
    Array,
    Calendar,
    Circle,
    Citation,
    CodeBlock,
    Cog,
    Cross,
    Database,
    DiagramTree,
    Error,
    Exchange,
    Export,
    Flows,
    FolderClose,
    Function,
    GanttChart,
    GlobeNetwork,
    History,
    IdNumber,
    Import,
    List,
    Lock,
    NewObject,
    Numerical,
    Pause,
    Play,
    Search,
    Tag,
    Tick,
    Time,
    Unlock,
}

impl Icon {
    /// Returns the Unicode character or emoji for this icon.
    #[must_use]
    pub fn as_char(self) -> &'static str {
        match self {
            Icon::Blank => " ",
            Icon::Antenna => "📡",
            Icon::Array => "[]",
            Icon::Calendar => "📅",
            Icon::Circle => "○",
            Icon::Citation => "❝",
            Icon::CodeBlock => "⌨",
            Icon::Cog => "⚙",
            Icon::Cross => "✗",
            Icon::Database => "🗄",
            Icon::DiagramTree => "🌳",
            Icon::Error => "⚠",
            Icon::Exchange => "⇄",
            Icon::Export => "↗",
            Icon::Flows => "↹",
            Icon::FolderClose => "📁",
            Icon::Function => "ƒ",
            Icon::GanttChart => "📊",
            Icon::GlobeNetwork => "🌐",
            Icon::History => "⏱",
            Icon::IdNumber => "#",
            Icon::Import => "↙",
            Icon::List => "☰",
            Icon::Lock => "🔒",
            Icon::NewObject => "✚",
            Icon::Numerical => "🔢",
            Icon::Pause => "⏸",
            Icon::Play => "▶",
            Icon::Search => "🔍",
            Icon::Tag => "🏷",
            Icon::Tick => "✓",
            Icon::Time => "⏰",
            Icon::Unlock => "🔓",
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct IconProps {
    pub icon: Icon,
    #[prop_or_default]
    pub class: Classes,
}

#[component(IconView)]
pub fn icon_view(props: &IconProps) -> Html {
    let mut classes = props.class.clone();
    classes.push("tree-icon");
    html! {
        <span class={classes} aria-hidden="true">
            { props.icon.as_char() }
        </span>
    }
}

impl From<Icon> for Html {
    fn from(icon: Icon) -> Html {
        html! { <IconView icon={icon} /> }
    }
}
