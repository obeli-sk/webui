use crate::{
    BASE_URL,
    app::{AppState, Route},
    components::{
        code::code_block::CodeBlock,
        notification::{Notification, NotificationContext},
    },
    grpc::{
        SUFFIX_FN_SCHEDULE, SUFFIX_PKG_SCHEDULE,
        ffqn::FunctionFqn,
        grpc_client::{self, ExecutionId},
        ifc_fqn::IfcFqn,
    },
    util::{wit_highlighter, wit_type_formatter::format_wit_type},
};
use log::{debug, error, trace, warn};
use serde_json::json;
use std::{collections::HashSet, ops::Deref};
use val_json::wast_val::WastValWithType;
use web_sys::{HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement};
use yew::prelude::*;

/// Returns current local time as "yyyy-mm-dd HH:MM:SS".
fn local_now() -> String {
    let d = js_sys::Date::new_0();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        d.get_full_year(),
        d.get_month() + 1,
        d.get_date(),
        d.get_hours(),
        d.get_minutes(),
        d.get_seconds(),
    )
}

/// Returns the browser's timezone offset in minutes (west of UTC, e.g. -60 for UTC+1).
fn tz_offset_minutes() -> i32 {
    js_sys::Date::new_0().get_timezone_offset() as i32
}

/// Returns the browser's IANA timezone name (e.g. "Europe/Berlin").
fn tz_name() -> String {
    let opts = js_sys::Intl::DateTimeFormat::new(&js_sys::Array::new(), &js_sys::Object::new())
        .resolved_options();
    js_sys::Reflect::get(&opts, &wasm_bindgen::JsValue::from_str("timeZone"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// Find the schedule-variant FFQN for a given function, if it exists.
fn find_schedule_ffqn(ffqn: &FunctionFqn, app_state: &AppState) -> Option<FunctionFqn> {
    let schedule_pkg_name = format!(
        "{pkg}{SUFFIX_PKG_SCHEDULE}",
        pkg = ffqn.ifc_fqn.pkg_fqn.package_name,
    );
    let schedule_ifc_fqn = IfcFqn {
        pkg_fqn: crate::grpc::pkg_fqn::PkgFqn {
            namespace: ffqn.ifc_fqn.pkg_fqn.namespace.clone(),
            package_name: schedule_pkg_name,
            version: ffqn.ifc_fqn.pkg_fqn.version.clone(),
        },
        ifc_name: ffqn.ifc_fqn.ifc_name.clone(),
    };
    let schedule_ffqn = FunctionFqn {
        ifc_fqn: schedule_ifc_fqn,
        function_name: format!(
            "{fn_name}{SUFFIX_FN_SCHEDULE}",
            fn_name = ffqn.function_name,
        ),
    };
    if app_state.ffqns_to_details.contains_key(&schedule_ffqn) {
        Some(schedule_ffqn)
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
struct FormData {
    param_refs: Vec<NodeRef>,
    param_errs: Vec<Option<String>>,
}

impl FormData {
    fn validate_param(
        function_detail: &grpc_client::FunctionDetail,
        param_value: &str,
        idx: usize,
    ) -> Result<(), String> {
        match serde_json::from_str::<serde_json::Value>(param_value) {
            Ok(param_value) => {
                let wit_type_inline = function_detail
                    .params
                    .get(idx)
                    .as_ref()
                    .expect("FunctionDetail.params cardinality must match FormData.param_refs")
                    .r#type
                    .as_ref()
                    .expect("`FunctionParameter.type` is sent")
                    .wit_type_inline
                    .as_str();
                let type_and_value_json = json!({
                    "type": wit_type_inline,
                    "value": param_value,
                });
                match serde_json::from_value::<WastValWithType>(type_and_value_json) {
                    Ok(_) => Ok(()),
                    Err(err) => {
                        warn!("oninput[{idx}] - typecheck error {err:?}");
                        Err(format!("Typecheck error: {err}"))
                    }
                }
            }
            Err(err) => {
                warn!("oninput[{idx}] - cannot serialize value to JSON - {err:?}");
                Err(format!("Cannot serialize value to JSON: {err}"))
            }
        }
    }

    fn validate(&mut self, function_detail: &grpc_client::FunctionDetail) -> Result<(), String> {
        let mut is_err = false;
        for (idx, param_ref) in self.param_refs.iter().enumerate() {
            let param_value = param_ref.cast::<HtmlTextAreaElement>().unwrap().value();
            debug!("oninput[{idx}] value {param_value}");
            if let Err(err) = Self::validate_param(function_detail, &param_value, idx) {
                self.param_errs[idx] = Some(err);
                is_err = true;
            } else {
                self.param_errs[idx] = None;
            }
        }
        if is_err {
            Err("Cannot serialize parameters".to_string())
        } else {
            Ok(())
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct ExecutionSubmitPageProps {
    pub ffqn: FunctionFqn,
}

#[component(ExecutionSubmitPage)]
pub fn execution_submit_page(ExecutionSubmitPageProps { ffqn }: &ExecutionSubmitPageProps) -> Html {
    let app_state =
        use_context::<AppState>().expect("AppState context is set when starting the App");
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let navigator = yew_router::hooks::use_navigator().unwrap();

    let (function_detail, component_id) = match app_state.ffqns_to_details.get(ffqn) {
        Some((detail, id)) => (detail.clone(), id.clone()),
        None => {
            return html! {
                <p>{"Function not found"}</p>
            };
        }
    };

    let schedule_ffqn = find_schedule_ffqn(ffqn, &app_state);
    let has_schedule = schedule_ffqn.is_some();

    // Form state
    let request_processing_state = use_state(|| false);
    let form_data_state = use_state(|| FormData {
        param_refs: std::iter::repeat_with(NodeRef::default)
            .take(function_detail.params.len())
            .collect(),
        param_errs: std::iter::repeat_n(None, function_detail.params.len()).collect(),
    });
    let validation_err_state = use_state(|| None::<String>);
    let paused_state = use_state(|| false);
    let expanded_type_hints = use_state(HashSet::<usize>::new);

    // Scheduling state
    let schedule_enabled = use_state(|| false);
    let schedule_mode = use_state(|| "in".to_string());
    let schedule_at_value = use_state(local_now);
    let schedule_at_tz = use_state(|| "local".to_string()); // "local" or "utc"
    let schedule_in_amount = use_state(|| "1".to_string());
    let schedule_in_unit = use_state(|| "hours".to_string());

    // WIT state
    let wit_state: UseStateHandle<Option<String>> = use_state(|| None);
    {
        let wit_state = wit_state.clone();
        let component_id = component_id.clone();
        use_effect_with(ffqn.clone(), move |_ffqn| {
            wasm_bindgen_futures::spawn_local(async move {
                let mut fn_client =
                    grpc_client::function_repository_client::FunctionRepositoryClient::new(
                        tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
                    );
                match fn_client
                    .get_wit(grpc_client::GetWitRequest {
                        component_digest: Some(
                            component_id.digest.clone().expect("`digest` is sent"),
                        ),
                        ..Default::default()
                    })
                    .await
                {
                    Ok(response) => wit_state.set(response.into_inner().content),
                    Err(e) => {
                        log::error!("Failed to fetch WIT: {}", e.message());
                    }
                }
            });
        });
    }

    // Validate on first render
    use_effect_with(form_data_state.deref().clone(), {
        let validation_err_state = validation_err_state.clone();
        let fn_detail = function_detail.clone();
        let form_data_state = form_data_state.clone();
        move |form_data| {
            let mut form_data = form_data.clone();
            if let Err(err) = form_data.validate(&fn_detail) {
                validation_err_state.set(Some(err));
            } else {
                validation_err_state.set(None);
            }
            form_data_state.set(form_data);
        }
    });

    // Build schedule-at JSON value
    let build_schedule_at_json = {
        let schedule_mode = schedule_mode.clone();
        let schedule_at_value = schedule_at_value.clone();
        let schedule_at_tz = schedule_at_tz.clone();
        let schedule_in_amount = schedule_in_amount.clone();
        let schedule_in_unit = schedule_in_unit.clone();
        move || -> Result<serde_json::Value, String> {
            match schedule_mode.as_str() {
                "now" => Ok(json!("now")),
                "at" => {
                    let datetime_str = (*schedule_at_value).clone();
                    if datetime_str.is_empty() {
                        return Err("Please enter a date and time".to_string());
                    }
                    let naive =
                        chrono::NaiveDateTime::parse_from_str(&datetime_str, "%Y-%m-%d %H:%M:%S")
                            .map_err(|e| {
                            format!("Invalid datetime (expected yyyy-mm-dd HH:MM:SS): {e}")
                        })?;
                    let seconds = if *schedule_at_tz == "utc" {
                        // Treat input as UTC directly
                        naive.and_utc().timestamp() as u64
                    } else {
                        // Convert local time to UTC using the browser's timezone offset
                        let offset_minutes = tz_offset_minutes();
                        let local_offset = chrono::FixedOffset::west_opt(offset_minutes * 60)
                            .ok_or_else(|| {
                                format!("Invalid timezone offset: {offset_minutes} minutes")
                            })?;
                        let local_datetime = naive
                            .and_local_timezone(local_offset)
                            .single()
                            .ok_or("Ambiguous local time")?;
                        local_datetime.to_utc().timestamp() as u64
                    };
                    Ok(json!({"at": {"seconds": seconds, "nanoseconds": 0}}))
                }
                "in" => {
                    let amount: u64 = schedule_in_amount
                        .parse()
                        .map_err(|_| "Invalid amount".to_string())?;
                    if amount == 0 {
                        return Err("Amount must be greater than 0".to_string());
                    }
                    let unit = (*schedule_in_unit).clone();
                    Ok(json!({"in": {unit: amount}}))
                }
                _ => Err("Invalid schedule mode".to_string()),
            }
        }
    };

    let on_submit = {
        let request_processing_state = request_processing_state.clone();
        let form_data_state = form_data_state.clone();
        let validation_err_state = validation_err_state.clone();
        let notifications = notifications.clone();
        let ffqn = ffqn.clone();
        let schedule_ffqn = schedule_ffqn.clone();
        let paused_state = paused_state.clone();
        let schedule_enabled = schedule_enabled.clone();
        let build_schedule_at_json = build_schedule_at_json.clone();
        let navigator = navigator.clone();
        Callback::from(move |e: SubmitEvent| {
            e.prevent_default();

            let params = match form_data_state
                .deref()
                .param_refs
                .iter()
                .enumerate()
                .map(|(idx, param_ref)| {
                    let param_value = param_ref.cast::<HtmlTextAreaElement>().unwrap().value();
                    serde_json::from_str(&param_value).map_err(|err| (idx, err))
                })
                .collect::<Result<Vec<serde_json::Value>, _>>()
            {
                Ok(params) => params,
                Err((idx, serde_err)) => {
                    error!("Cannot serialize parameters - {serde_err:?}");
                    validation_err_state.set(Some(format!(
                        "cannot serialize {idx}-th parameter - {serde_err}"
                    )));
                    return;
                }
            };

            let (submit_ffqn, submit_params) = if *schedule_enabled {
                let schedule_ffqn = match schedule_ffqn.clone() {
                    Some(f) => f,
                    None => return,
                };
                let schedule_at = match build_schedule_at_json() {
                    Ok(v) => v,
                    Err(err) => {
                        validation_err_state.set(Some(err));
                        return;
                    }
                };
                let mut all_params = vec![schedule_at];
                all_params.extend(params);
                (schedule_ffqn, all_params)
            } else {
                (ffqn.clone(), params)
            };

            validation_err_state.set(None);
            request_processing_state.set(true);

            wasm_bindgen_futures::spawn_local({
                let notifications = notifications.clone();
                let navigator = navigator.clone();
                let request_processing_state = request_processing_state.clone();
                let paused = *paused_state;
                async move {
                    let mut client =
                        grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
                            tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
                        );
                    let execution_id = ExecutionId::generate();
                    let params_json = serde_json::Value::Array(submit_params);
                    let type_url = format!("urn:obelisk:json:params:{submit_ffqn}");
                    log::info!(
                        "Submitting execution: ffqn={submit_ffqn}, type_url={type_url}, params={params_json}, paused={paused}"
                    );
                    let response = client
                        .submit(grpc_client::SubmitRequest {
                            execution_id: Some(execution_id.clone()),
                            params: Some(prost_wkt_types::Any {
                                type_url,
                                value: params_json.to_string().into_bytes(),
                            }),
                            function_name: Some(grpc_client::FunctionName::from(submit_ffqn)),
                            paused,
                        })
                        .await;
                    request_processing_state.set(false);
                    trace!("Got gRPC {response:?}");
                    match response {
                        Ok(_response) => navigator.push(&Route::ExecutionTrace { execution_id }),
                        Err(err) => {
                            error!("Got error {err:?}");
                            notifications.push(Notification::error(format!(
                                "Cannot submit the execution: {err}"
                            )));
                        }
                    }
                }
            });
        })
    };

    // Render parameter fields
    let params_html: Vec<_> = function_detail
        .params
        .iter()
        .enumerate()
        .map(|(idx, param)| {
            let ty = param
                .r#type
                .as_ref()
                .expect("`FunctionParameter.type` is sent");
            let id = format!("param_{ffqn}_{idx}");

            let on_param_change = {
                let form_data_state = form_data_state.clone();
                let validation_err_state = validation_err_state.clone();
                let fn_detail = function_detail.clone();
                move || {
                    let mut form_data = form_data_state.deref().clone();
                    if let Err(err) = form_data.validate(&fn_detail) {
                        validation_err_state.set(Some(err));
                    } else {
                        validation_err_state.set(None);
                    }
                    form_data_state.set(form_data);
                }
            };

            let is_expanded = expanded_type_hints.contains(&idx);
            let on_toggle_type = {
                let expanded_type_hints = expanded_type_hints.clone();
                Callback::from(move |_: MouseEvent| {
                    let mut set = (*expanded_type_hints).clone();
                    if set.contains(&idx) {
                        set.remove(&idx);
                    } else {
                        set.insert(idx);
                    }
                    expanded_type_hints.set(set);
                })
            };
            let wit_type_formatted = format_wit_type(&ty.wit_type_inline);

            html! {
                <div class="form-field">
                    <div class="form-field-row">
                        <label for={id.clone()}>
                            {format!("{}:", &param.name)}
                        </label>
                        <textarea
                            id={id}
                            rows="1"
                            placeholder={ty.wit_type.clone()}
                            ref={&form_data_state.param_refs[idx]}
                            oninput={Callback::from(move |_| {
                                on_param_change()
                            })}
                        />
                        <span
                            class="wit-type-toggle"
                            onclick={on_toggle_type}
                            title="Show full type"
                        >
                            {"i"}
                        </span>
                        if let Some(Some(err)) = form_data_state.param_errs.get(idx) {
                            <span class="validation-error">{err.clone()}</span>
                        }
                    </div>
                    if is_expanded {
                        <pre class="wit-type-inline">{wit_type_formatted}</pre>
                    }
                </div>
            }
        })
        .collect();

    let on_paused_change = {
        let paused_state = paused_state.clone();
        Callback::from(move |e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            paused_state.set(input.checked());
        })
    };

    let on_schedule_toggle = {
        let schedule_enabled = schedule_enabled.clone();
        Callback::from(move |e: Event| {
            let input: web_sys::HtmlInputElement = e.target_unchecked_into();
            schedule_enabled.set(input.checked());
        })
    };

    let on_schedule_mode_change = {
        let schedule_mode = schedule_mode.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target_unchecked_into();
            schedule_mode.set(select.value());
        })
    };

    let on_schedule_at_change = {
        let schedule_at_value = schedule_at_value.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            schedule_at_value.set(input.value());
        })
    };

    let on_schedule_at_tz_change = {
        let schedule_at_tz = schedule_at_tz.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target_unchecked_into();
            schedule_at_tz.set(select.value());
        })
    };

    let on_schedule_in_amount_change = {
        let schedule_in_amount = schedule_in_amount.clone();
        Callback::from(move |e: InputEvent| {
            let input: HtmlInputElement = e.target_unchecked_into();
            schedule_in_amount.set(input.value());
        })
    };

    let on_schedule_in_unit_change = {
        let schedule_in_unit = schedule_in_unit.clone();
        Callback::from(move |e: Event| {
            let select: HtmlSelectElement = e.target_unchecked_into();
            schedule_in_unit.set(select.value());
        })
    };

    let wit = wit_state
        .deref()
        .as_ref()
        .map(|wit| wit_highlighter::print_interface_with_single_fn(wit, ffqn));

    html! {<>
        <header>
            <h1>{"Execution submit"}</h1>
            <h2>
                {ffqn.to_string()}
            </h2>
        </header>

        <form id="execution-submit-form" onsubmit={on_submit}>
            {for params_html}
            if let Some(err) = validation_err_state.deref() {
                <div class="validation-error">{err}</div>
            }
            <div class="form-field">
                <label>
                    <input
                        type="checkbox"
                        checked={*paused_state}
                        onchange={on_paused_change}
                    />
                    {" Create paused"}
                </label>
            </div>

            if has_schedule {
                <div class="form-field schedule-section">
                    <label>
                        <input
                            type="checkbox"
                            checked={*schedule_enabled}
                            onchange={on_schedule_toggle}
                        />
                        {" Schedule execution"}
                    </label>
                    if *schedule_enabled {
                        <div class="schedule-options">
                            <div class="schedule-mode-row">
                                <label for="schedule-mode">{"When:"}</label>
                                <select
                                    id="schedule-mode"
                                    onchange={on_schedule_mode_change}
                                >
                                    <option value="in" selected={*schedule_mode == "in"}>
                                        {"In (relative)"}
                                    </option>
                                    <option value="at" selected={*schedule_mode == "at"}>
                                        {"At (absolute)"}
                                    </option>
                                    <option value="now" selected={*schedule_mode == "now"}>
                                        {"Now"}
                                    </option>
                                </select>
                            </div>
                            if *schedule_mode == "at" {
                                <div class="schedule-detail-row">
                                    <label for="schedule-at-datetime">{"Date/Time:"}</label>
                                    <input
                                        id="schedule-at-datetime"
                                        type="text"
                                        placeholder="yyyy-mm-dd HH:MM:SS"
                                        value={(*schedule_at_value).clone()}
                                        oninput={on_schedule_at_change}
                                    />
                                    <select onchange={on_schedule_at_tz_change}>
                                        <option value="local" selected={*schedule_at_tz == "local"}>
                                            {tz_name()}
                                        </option>
                                        <option value="utc" selected={*schedule_at_tz == "utc"}>
                                            {"UTC"}
                                        </option>
                                    </select>
                                </div>
                            }
                            if *schedule_mode == "in" {
                                <div class="schedule-detail-row">
                                    <label for="schedule-in-amount">{"Delay:"}</label>
                                    <input
                                        id="schedule-in-amount"
                                        type="number"
                                        min="1"
                                        value={(*schedule_in_amount).clone()}
                                        oninput={on_schedule_in_amount_change}
                                    />
                                    <select onchange={on_schedule_in_unit_change}>
                                        <option value="minutes" selected={*schedule_in_unit == "minutes"}>
                                            {"minutes"}
                                        </option>
                                        <option value="hours" selected={*schedule_in_unit == "hours"}>
                                            {"hours"}
                                        </option>
                                        <option value="days" selected={*schedule_in_unit == "days"}>
                                            {"days"}
                                        </option>
                                        <option value="seconds" selected={*schedule_in_unit == "seconds"}>
                                            {"seconds"}
                                        </option>
                                    </select>
                                </div>
                            }
                        </div>
                    }
                </div>
            }

            <button
                type="submit"
                disabled={*request_processing_state || validation_err_state.is_some()}
            >
                if *schedule_enabled {
                    {"Schedule"}
                } else {
                    {"Submit"}
                }
            </button>
        </form>

        if let Some(Ok(wit)) = wit {
            <h3>{"WIT"}</h3>
            <CodeBlock source={wit.clone()} />
        }
    </>}
}
