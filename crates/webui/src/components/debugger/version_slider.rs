use crate::grpc::version::VersionType;
use std::collections::BTreeSet;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct VersionSliderProps {
    /// All versions that have backtraces, in sorted order
    pub backtrace_versions: BTreeSet<VersionType>,
    /// Currently selected version
    pub selected_version: VersionType,
    /// Callback when a new version is selected
    pub on_version_change: Callback<VersionType>,
}

/// A horizontal slider component for selecting backtrace versions.
/// Displays tick marks for each available version and allows drag-and-drop selection.
#[function_component(VersionSlider)]
pub fn version_slider(
    VersionSliderProps {
        backtrace_versions,
        selected_version,
        on_version_change,
    }: &VersionSliderProps,
) -> Html {
    let versions: Vec<VersionType> = backtrace_versions.iter().copied().collect();

    if versions.is_empty() {
        return html! {
            <div class="version-slider-empty">
                {"No backtrace versions available"}
            </div>
        };
    }

    // Find the index of the currently selected version (or closest)
    let selected_index = versions
        .iter()
        .position(|&v| v >= *selected_version)
        .unwrap_or(versions.len() - 1);

    let max_index = versions.len().saturating_sub(1);

    // Handle range input change
    let on_input = {
        let versions = versions.clone();
        let on_version_change = on_version_change.clone();
        Callback::from(move |e: InputEvent| {
            if let Some(input) = e.target_dyn_into::<web_sys::HtmlInputElement>()
                && let Ok(index) = input.value().parse::<usize>()
                && let Some(&version) = versions.get(index)
            {
                on_version_change.emit(version);
            }
        })
    };

    // Calculate display info
    let min_version = versions.first().copied().unwrap_or(0);
    let max_version = versions.last().copied().unwrap_or(0);

    html! {
        <div class="version-slider-container">
            <div class="version-slider-header">
                <span class="version-slider-label">{"Version:"}</span>
                <span class="version-slider-current">{selected_version}</span>
                <span class="version-slider-range">
                    {format!("({} - {})", min_version, max_version)}
                </span>
            </div>
            <div class="version-slider-track-container">
                <input
                    type="range"
                    class="version-slider-input"
                    min="0"
                    max={max_index.to_string()}
                    value={selected_index.to_string()}
                    oninput={on_input}
                />
                <div class="version-slider-ticks">
                    {versions.iter().enumerate().map(|(i, &v)| {
                        let is_selected = v == *selected_version;
                        let position_pct = if max_index > 0 {
                            (i as f64 / max_index as f64) * 100.0
                        } else {
                            50.0
                        };
                        html! {
                            <div
                                class={classes!("version-slider-tick", is_selected.then_some("selected"))}
                                style={format!("left: {}%;", position_pct)}
                                title={format!("Version {}", v)}
                            />
                        }
                    }).collect::<Html>()}
                </div>
            </div>
            <div class="version-slider-labels">
                <span class="version-slider-min">{min_version}</span>
                <span class="version-slider-max">{max_version}</span>
            </div>
        </div>
    }
}
