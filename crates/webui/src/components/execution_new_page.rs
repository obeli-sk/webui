use crate::{
    app::{AppState, Route},
    grpc::{ffqn::FunctionFqn, grpc_client::ComponentType},
};
use hashbrown::{HashMap, HashSet};
use std::{collections::BTreeMap, str::FromStr};
use yew::prelude::*;
use yew_router::hooks::use_navigator;

#[derive(Clone, PartialEq)]
struct FunctionPickerData {
    packages: Vec<String>,
    interfaces_by_package: BTreeMap<String, Vec<String>>,
    functions_by_interface: BTreeMap<String, Vec<String>>,
}

fn build_function_picker_data(
    app_state: &AppState,
    show_workflows: bool,
    show_activities: bool,
) -> FunctionPickerData {
    let mut packages = HashSet::new();
    let mut interfaces_by_package: HashMap<String, HashSet<String>> = HashMap::new();
    let mut functions_by_interface: HashMap<String, HashSet<String>> = HashMap::new();

    for (ffqn, (function_detail, component_id)) in &app_state.ffqns_to_details {
        let component_type = component_id.component_type();
        let type_is_visible = match component_type {
            ComponentType::Workflow => show_workflows,
            ComponentType::Activity => show_activities,
            _ => false,
        };

        if !function_detail.submittable || !type_is_visible || ffqn.ifc_fqn.pkg_fqn.is_extension() {
            continue;
        }

        let package = ffqn.ifc_fqn.pkg_fqn.to_string();
        let interface = ffqn.ifc_fqn.to_string();
        let function = ffqn.to_string();

        packages.insert(package.clone());
        interfaces_by_package
            .entry(package)
            .or_default()
            .insert(interface.clone());
        functions_by_interface
            .entry(interface)
            .or_default()
            .insert(function);
    }

    let mut packages = packages.into_iter().collect::<Vec<_>>();
    packages.sort();

    let interfaces_by_package = interfaces_by_package
        .into_iter()
        .map(|(package, interfaces)| {
            let mut interfaces = interfaces.into_iter().collect::<Vec<_>>();
            interfaces.sort();
            (package, interfaces)
        })
        .collect::<BTreeMap<_, _>>();

    let functions_by_interface = functions_by_interface
        .into_iter()
        .map(|(interface, functions)| {
            let mut functions = functions.into_iter().collect::<Vec<_>>();
            functions.sort();
            (interface, functions)
        })
        .collect::<BTreeMap<_, _>>();

    FunctionPickerData {
        packages,
        interfaces_by_package,
        functions_by_interface,
    }
}

#[component(ExecutionNewPage)]
pub fn execution_new_page() -> Html {
    let app_state =
        use_context::<AppState>().expect("AppState context is set when starting the App");
    let navigator = use_navigator().expect("should be called inside a router");

    let show_workflows = use_state(|| true);
    let show_activities = use_state(|| true);
    let selected_package = use_state(|| None::<String>);
    let selected_interface = use_state(|| None::<String>);

    let picker_data = build_function_picker_data(&app_state, *show_workflows, *show_activities);
    let selected_package_value = (*selected_package).clone();
    let selected_interface_value = (*selected_interface).clone();

    let on_toggle_workflows = {
        let show_workflows = show_workflows.clone();
        let selected_package = selected_package.clone();
        let selected_interface = selected_interface.clone();
        Callback::from(move |_| {
            show_workflows.set(!*show_workflows);
            selected_package.set(None);
            selected_interface.set(None);
        })
    };
    let on_toggle_activities = {
        let show_activities = show_activities.clone();
        let selected_package = selected_package.clone();
        let selected_interface = selected_interface.clone();
        Callback::from(move |_| {
            show_activities.set(!*show_activities);
            selected_package.set(None);
            selected_interface.set(None);
        })
    };

    let interfaces = selected_package_value
        .as_ref()
        .and_then(|package| picker_data.interfaces_by_package.get(package))
        .cloned()
        .unwrap_or_default();
    let functions = selected_interface_value
        .as_ref()
        .and_then(|interface| picker_data.functions_by_interface.get(interface))
        .cloned()
        .unwrap_or_default();

    html! {<>
        <header>
            <h1>{"Submit Execution"}</h1>
            <h3>{"Select a function"}</h3>
        </header>

        <div class="function-picker-filters">
            <span class="function-picker-filter-label">{"Show:"}</span>
            <label>
                <input
                    type="checkbox"
                    checked={*show_workflows}
                    onchange={on_toggle_workflows}
                />
                {" Workflows"}
            </label>
            <label>
                <input
                    type="checkbox"
                    checked={*show_activities}
                    onchange={on_toggle_activities}
                />
                {" Activities"}
            </label>
        </div>

        <div class="function-picker-columns">
            <div class="function-picker-column">
                <div class="function-picker-column-title">{"Packages"}</div>
                <div class="function-picker-list">
                    {for picker_data.packages.iter().map(|package| {
                        let package_value = package.clone();
                        let selected_package = selected_package.clone();
                        let selected_interface = selected_interface.clone();
                        let is_selected = selected_package_value.as_ref() == Some(package);
                        let onclick = Callback::from(move |_| {
                            selected_package.set(Some(package_value.clone()));
                            selected_interface.set(None);
                        });
                        html! {
                            <button
                                type="button"
                                class={classes!("function-picker-option", is_selected.then_some("selected"))}
                                {onclick}
                            >
                                {package}
                            </button>
                        }
                    })}
                </div>
            </div>

            <div class="function-picker-column">
                <div class="function-picker-column-title">{"Interfaces"}</div>
                <div class="function-picker-list">
                    {for interfaces.iter().map(|interface| {
                        let interface_value = interface.clone();
                        let selected_interface = selected_interface.clone();
                        let is_selected = selected_interface_value.as_ref() == Some(interface);
                        let onclick = Callback::from(move |_| {
                            selected_interface.set(Some(interface_value.clone()));
                        });
                        html! {
                            <button
                                type="button"
                                class={classes!("function-picker-option", is_selected.then_some("selected"))}
                                {onclick}
                            >
                                {interface}
                            </button>
                        }
                    })}
                </div>
            </div>

            <div class="function-picker-column">
                <div class="function-picker-column-title">{"Functions"}</div>
                <div class="function-picker-list">
                    {for functions.iter().map(|function| {
                        let ffqn = FunctionFqn::from_str(function);
                        let function_name = ffqn.as_ref()
                            .map(|f| f.function_name.clone())
                            .unwrap_or_else(|_| function.clone());
                        let navigator = navigator.clone();
                        let onclick = Callback::from(move |_| {
                            if let Ok(ref ffqn) = ffqn {
                                navigator.push(&Route::ExecutionSubmit { ffqn: ffqn.clone() });
                            }
                        });
                        html! {
                            <button
                                type="button"
                                class="function-picker-option"
                                {onclick}
                            >
                                {function_name}
                            </button>
                        }
                    })}
                </div>
            </div>
        </div>
    </>}
}
