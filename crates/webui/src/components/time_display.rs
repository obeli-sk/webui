use crate::util::time::{RelativeAgo, format_date};
use chrono::{DateTime, Utc};
use wasm_bindgen::JsValue;
use yew::prelude::*;

const STORAGE_KEY: &str = "obelisk-webui-time-mode";

#[derive(Clone, Copy, PartialEq, Default)]
pub enum TimeMode {
    Relative,
    Local,
    #[default]
    Utc,
}

impl TimeMode {
    const ALL: [Self; 3] = [Self::Relative, Self::Local, Self::Utc];

    fn label(self) -> &'static str {
        match self {
            Self::Relative => "Relative",
            Self::Local => "Local",
            Self::Utc => "UTC",
        }
    }

    fn from_storage(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.label() == value)
    }
}

#[derive(Clone, PartialEq)]
struct TimeDisplayContext {
    mode: Option<TimeMode>,
    set_mode: Callback<TimeMode>,
}

#[derive(Properties, PartialEq)]
pub struct TimeDisplayProviderProps {
    pub children: Children,
}

#[component(TimeDisplayProvider)]
pub fn time_display_provider(props: &TimeDisplayProviderProps) -> Html {
    let mode = use_state(|| {
        web_sys::window()
            .and_then(|window| window.local_storage().ok().flatten())
            .and_then(|storage| storage.get_item(STORAGE_KEY).ok().flatten())
            .and_then(|value| TimeMode::from_storage(&value))
    });
    let set_mode = {
        let mode = mode.clone();
        Callback::from(move |selected: TimeMode| {
            if let Some(storage) =
                web_sys::window().and_then(|window| window.local_storage().ok().flatten())
            {
                let _ = storage.set_item(STORAGE_KEY, selected.label());
            }
            mode.set(Some(selected));
        })
    };
    let context = TimeDisplayContext {
        mode: *mode,
        set_mode,
    };
    html! {
        <ContextProvider<TimeDisplayContext> {context}>
            {props.children.clone()}
        </ContextProvider<TimeDisplayContext>>
    }
}

#[derive(Properties, PartialEq)]
pub struct TimeDisplayControlProps {
    #[prop_or_default]
    pub default_mode: TimeMode,
}

#[component(TimeDisplayControl)]
pub fn time_display_control(props: &TimeDisplayControlProps) -> Html {
    let context = use_context::<TimeDisplayContext>().expect("TimeDisplayProvider is present");
    let selected = context.mode.unwrap_or(props.default_mode);
    html! {
        <div class="time-display-control" role="group" aria-label="Timestamp display">
            {for TimeMode::ALL.into_iter().map(|mode| {
                let set_mode = context.set_mode.clone();
                html! {
                    <button type="button" class={classes!((selected == mode).then_some("selected"))}
                        aria-pressed={(selected == mode).to_string()}
                        onclick={Callback::from(move |_| set_mode.emit(mode))}>
                        {mode.label()}
                    </button>
                }
            })}
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct TimestampProps {
    pub target: DateTime<Utc>,
    #[prop_or_default]
    pub default_mode: TimeMode,
}

fn format_local(target: DateTime<Utc>) -> String {
    format_browser_time(target, None)
}

fn format_browser_time(target: DateTime<Utc>, time_zone: Option<&str>) -> String {
    let date = js_sys::Date::new(&JsValue::from_str(&target.to_rfc3339()));
    let options = js_sys::JSON::parse(
        r#"{"year":"numeric","month":"2-digit","day":"2-digit","hour":"2-digit","minute":"2-digit","second":"2-digit","fractionalSecondDigits":3,"hour12":false,"timeZoneName":"shortOffset"}"#,
    ).expect("valid date formatting options");
    if let Some(time_zone) = time_zone {
        js_sys::Reflect::set(
            &options,
            &JsValue::from_str("timeZone"),
            &JsValue::from_str(time_zone),
        )
        .expect("date options can be set");
    }
    date.to_locale_string("sv-SE", &options).into()
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn local_timestamps_apply_offsets_and_daylight_saving() {
        let winter = DateTime::parse_from_rfc3339("2026-01-02T23:30:08.123Z")
            .unwrap()
            .to_utc();
        let summer = DateTime::parse_from_rfc3339("2026-07-02T23:30:08.123Z")
            .unwrap()
            .to_utc();
        let winter = format_browser_time(winter, Some("Europe/Prague"));
        let summer = format_browser_time(summer, Some("Europe/Prague"));
        assert!(winter.contains("2026-01-03"), "{winter}");
        assert!(winter.contains("00:30:08,123"), "{winter}");
        assert!(winter.contains("GMT+1"), "{winter}");
        assert!(summer.contains("2026-07-03"), "{summer}");
        assert!(summer.contains("01:30:08,123"), "{summer}");
        assert!(summer.contains("GMT+2"), "{summer}");
    }
}

#[component(Timestamp)]
pub fn timestamp(props: &TimestampProps) -> Html {
    let context = use_context::<TimeDisplayContext>();
    let mode = context
        .and_then(|context| context.mode)
        .unwrap_or(props.default_mode);
    let utc = format!("{} UTC", format_date(props.target));
    let local = format_local(props.target);
    let title = format!("UTC: {utc}\nLocal: {local}");
    html! {
        <time class="timestamp" datetime={props.target.to_rfc3339()} {title}>
            {match mode {
                TimeMode::Relative => html! { <RelativeAgo target={props.target} /> },
                TimeMode::Local => html! { {local} },
                TimeMode::Utc => html! { {utc} },
            }}
        </time>
    }
}
