use crate::{
    app::Route,
    components::{
        execution_list_page::{ExecutionQuery, StatusFilter, StatusFilterList},
        notification::{Notification, NotificationContext},
    },
    rest,
};
use concepts::prefixed_ulid::DeploymentId;
use log::error;
use rest_types::deployment::DeploymentStateSer;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlInputElement;
use yew::prelude::*;
use yew_router::prelude::*;

#[derive(Properties, PartialEq)]
pub struct DeploymentExecutionsProps {
    pub deployment_id: DeploymentId,
    /// The deployment has no components, so it cannot have executions.
    #[prop_or_default]
    pub empty: bool,
    /// Bumped to refetch the counts.
    #[prop_or_default]
    pub refresh: u32,
}

/// Execution counts of a deployment by status, each linking to the filtered execution list.
#[component(DeploymentExecutions)]
pub fn deployment_executions(
    DeploymentExecutionsProps {
        deployment_id,
        empty,
        refresh,
    }: &DeploymentExecutionsProps,
) -> Html {
    let notifications =
        use_context::<NotificationContext>().expect("NotificationContext should be provided");
    let summary = use_state(|| None::<DeploymentStateSer>);
    let show_derived = use_state(|| false);

    {
        let summary = summary.clone();
        use_effect_with(
            (*deployment_id, *show_derived, *refresh),
            move |(deployment_id, show_derived, _)| {
                let deployment_id = *deployment_id;
                let show_derived = *show_derived;
                summary.set(None);
                spawn_local(async move {
                    let response = rest::deployments::list(
                        Some(&deployment_id),
                        "older",
                        1,
                        true,
                        show_derived,
                        false,
                    )
                    .await;
                    match response {
                        Ok(response) => summary.set(
                            response
                                .into_iter()
                                .find(|summary| summary.deployment_id == deployment_id),
                        ),
                        Err(e) => {
                            error!("Failed to load deployment execution summary: {e:?}");
                            notifications.push(Notification::error(format!(
                                "Failed to load deployment execution summary: {e}"
                            )));
                        }
                    }
                });
            },
        );
    }

    let summary_html = summary.as_ref().map(|summary| {
        let total = summary.locked
            + summary.pending
            + summary.scheduled
            + summary.blocked
            + summary.paused
            + summary.cancelling
            + summary.finished_ok
            + summary.finished_error
            + summary.finished_execution_failure;
        let count_metric = |label: &'static str,
                            count: u32,
                            status: Option<StatusFilterList>| {
            let query = ExecutionQuery {
                deployment_id: Some(deployment_id.to_string()),
                status,
                show_derived: *show_derived,
                ..Default::default()
            };
            html! {
                <div class={classes!("execution-metric", (count == 0).then_some("empty"))}>
                    <span>{label}</span>
                    <strong>
                        if count > 0 {
                            <Link<Route, ExecutionQuery> to={Route::ExecutionList} query={query}>
                                {count}
                            </Link<Route, ExecutionQuery>>
                        } else {
                            {count}
                        }
                    </strong>
                </div>
            }
        };
        html! {
            <div class="deployment-execution-summary">
                { count_metric("Total", total, None) }
                { count_metric("In progress", summary.locked + summary.pending + summary.blocked, Some(StatusFilterList::in_progress())) }
                { count_metric("Scheduled", summary.scheduled, Some(StatusFilterList::single(StatusFilter::Scheduled))) }
                { count_metric("Paused", summary.paused, Some(StatusFilterList::single(StatusFilter::Paused))) }
                { count_metric("Cancelling", summary.cancelling, Some(StatusFilterList::single(StatusFilter::Cancelling))) }
                { count_metric("Successful", summary.finished_ok, Some(StatusFilterList::single(StatusFilter::FinishedOk))) }
                { count_metric("Errors", summary.finished_error, Some(StatusFilterList::single(StatusFilter::FinishedError))) }
                { count_metric("Failures", summary.finished_execution_failure, Some(StatusFilterList::single(StatusFilter::FinishedExecutionFailure))) }
            </div>
        }
    });

    let on_toggle_derived = {
        let show_derived = show_derived.clone();
        Callback::from(move |event: Event| {
            let input: HtmlInputElement = event.target_unchecked_into();
            show_derived.set(input.checked());
        })
    };

    html! {
        <section class="deployment-executions">
            <div class="deployment-section-heading">
                <h4>{"Executions"}</h4>
                if !empty {
                    <label>
                        <input type="checkbox" checked={*show_derived} onchange={on_toggle_derived} />
                        {" Include derived"}
                    </label>
                }
            </div>
            if *empty {
                <p class="secondary-text">{"This deployment contains no components."}</p>
            } else if let Some(summary_html) = summary_html {
                { summary_html }
            } else {
                <p>{"Loading execution summary..."}</p>
            }
        </section>
    }
}
