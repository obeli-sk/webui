use crate::{
    BASE_URL,
    components::execution_detail::finished::FinishedEvent,
    grpc::grpc_client::{
        self, ExecutionStatus as GExecutionStatus, ExecutionSummary, FinishedStatus,
        execution_status::{Finished, Locked, PendingAt},
        get_status_response,
    },
    util::trace_id,
};
use chrono::DateTime;
use futures::FutureExt as _;
use hashbrown::HashMap;
use log::{debug, error, trace};
use std::rc::Rc;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct ExecutionStatusProps {
    pub status: Option<grpc_client::execution_status::Status>,
    pub execution_id: grpc_client::ExecutionId,
    pub print_finished_status: bool,
}

fn status_as_message(
    status: Option<&grpc_client::execution_status::Status>,
) -> Option<get_status_response::Message> {
    status.map(|s| {
        get_status_response::Message::CurrentStatus(GExecutionStatus {
            status: Some(s.clone()),
            component_digest: None, // not displayed, not available
        })
    })
}

pub enum StatusStateAction {
    Update {
        execution_id: grpc_client::ExecutionId,
        message: get_status_response::Message,
    },
}

#[derive(Default, PartialEq)]
pub struct StatusState {
    pub statuses: HashMap<grpc_client::ExecutionId, get_status_response::Message>,
}

impl Reducible for StatusState {
    type Action = StatusStateAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            StatusStateAction::Update {
                execution_id,
                message,
            } => {
                let mut statuses = self.statuses.clone();
                statuses.insert(execution_id, message);
                Self { statuses }.into()
            }
        }
    }
}

pub type StatusCacheContext = UseReducerHandle<StatusState>;

fn is_finished_detailed(msg: &get_status_response::Message) -> bool {
    matches!(msg, get_status_response::Message::FinishedStatus(_))
}

fn is_finished_any(msg: &get_status_response::Message) -> bool {
    matches!(
        msg,
        get_status_response::Message::FinishedStatus(_)
            | get_status_response::Message::CurrentStatus(GExecutionStatus {
                status: Some(grpc_client::execution_status::Status::Finished(_)),
                ..
            })
            | get_status_response::Message::Summary(ExecutionSummary {
                current_status: Some(GExecutionStatus {
                    status: Some(grpc_client::execution_status::Status::Finished(_)),
                    ..
                }),
                ..
            })
    )
}

async fn run_status_subscription(
    status_state: UseReducerHandle<StatusState>,
    connection_id: Rc<str>,
    execution_id: grpc_client::ExecutionId,
    print_finished_status: bool,
    cancel_rx: futures::channel::oneshot::Receiver<()>,
) {
    let mut execution_client =
        grpc_client::execution_repository_client::ExecutionRepositoryClient::new(
            tonic_web_wasm_client::Client::new(BASE_URL.to_string()),
        );
    let mut response_stream = execution_client
        .get_status(grpc_client::GetStatusRequest {
            execution_id: Some(execution_id.clone()),
            follow: true,
            send_finished_status: print_finished_status,
        })
        .await
        .unwrap()
        .into_inner();
    let mut cancel_rx = cancel_rx.fuse();
    loop {
        let next_message = futures::select! {
            next_message = response_stream.message().fuse() => next_message,
            _ =  &mut cancel_rx => break,
        };
        match next_message {
            Ok(Some(status)) => {
                let status = status
                    .message
                    .expect("GetStatusResponse.message is sent by the server");
                trace!("[{connection_id}] <ExecutionStatus /> Got {status:?}");
                status_state.dispatch(StatusStateAction::Update {
                    execution_id: execution_id.clone(),
                    message: status,
                });
            }
            Ok(None) => break,
            Err(err) => {
                error!("[{connection_id}] Error wile listening to status updates: {err:?}");
                break;
            }
        }
    }
    debug!("[{connection_id}] <ExecutionStatus /> Ended subscription");
}

#[function_component(ExecutionStatus)]
pub fn execution_status(
    ExecutionStatusProps {
        status,
        execution_id,
        print_finished_status,
    }: &ExecutionStatusProps,
) -> Html {
    let print_finished_status = *print_finished_status;

    // Both hooks must be called unconditionally
    let context_state = use_context::<StatusCacheContext>();
    let local_state = use_reducer_eq(StatusState::default);

    // Prefer shared context if available, otherwise use local state
    let status_state = context_state.unwrap_or(local_state);

    // Sync status from props to state (if provided and better than current)
    use_effect_with((execution_id.clone(), status.clone()), {
        let status_state = status_state.clone();
        move |(execution_id, status)| {
            if let Some(msg) = status_as_message(status.as_ref()) {
                status_state.dispatch(StatusStateAction::Update {
                    execution_id: execution_id.clone(),
                    message: msg,
                });
            }
        }
    });

    let stored_message = status_state.statuses.get(execution_id).cloned();

    // Determine if we have a sufficient finished status to avoid re-subscribing.
    let is_done = if let Some(msg) = &stored_message {
        if print_finished_status {
            is_finished_detailed(msg)
        } else {
            is_finished_any(msg)
        }
    } else {
        false
    };

    // Subscription Effect
    {
        let status_state = status_state.clone();
        let execution_id = execution_id.clone();
        let connection_id = trace_id();

        use_effect_with(
            (execution_id.clone(), is_done),
            move |(execution_id, is_done)| {
                let execution_id = execution_id.clone();
                let cancel_tx = if *is_done {
                    trace!(
                        "[{connection_id}] Execution {execution_id} status is finished. Skipping subscription."
                    );
                    None
                } else {
                    let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel();
                    debug!(
                        "[{connection_id}] <ExecutionStatus /> Subscribing to status of {execution_id}"
                    );

                    wasm_bindgen_futures::spawn_local(run_status_subscription(
                        status_state,
                        connection_id,
                        execution_id.clone(),
                        print_finished_status,
                        cancel_rx,
                    ));
                    Some(cancel_tx)
                };

                move || {
                    if let Some(cancel_tx) = cancel_tx {
                        trace!("Cleaning up {execution_id}");
                        let _ = cancel_tx.send(());
                    }
                }
            },
        );
    }

    match stored_message.as_ref() {
        None => {
            html! {
                {"Loading..."}
            }
        }
        Some(get_status_response::Message::Summary(ExecutionSummary {
            current_status:
                Some(GExecutionStatus {
                    status: Some(status),
                    component_digest: _,
                }),
            ..
        }))
        | Some(get_status_response::Message::CurrentStatus(GExecutionStatus {
            status: Some(status),
            component_digest: _,
        })) => status_to_string(status),
        Some(get_status_response::Message::FinishedStatus(FinishedStatus {
            created_at: _,
            scheduled_at: Some(scheduled_at),
            finished_at: Some(finished_at),
            value: Some(result_detail),
        })) => {
            let finished_at = DateTime::from(*finished_at);
            let scheduled_at = DateTime::from(*scheduled_at);
            let since_scheduled = (finished_at - scheduled_at)
                .to_std()
                .expect("must be non-negative");
            html! {<>
                <FinishedEvent result_detail={result_detail.clone()} version={None} is_selected={false}/>
                <p>{format!("Execution completed in {since_scheduled:?}.")}</p>
            </>}
        }
        Some(unknown) => unreachable!("unexpected {unknown:?}"),
    }
}

pub fn status_to_string(status: &grpc_client::execution_status::Status) -> Html {
    match status {
        grpc_client::execution_status::Status::Locked(Locked {
            lock_expires_at, ..
        }) => html! {
            format!("Locked{}", convert_date(" until ", lock_expires_at.as_ref()))
        },
        grpc_client::execution_status::Status::PendingAt(PendingAt { scheduled_at }) => html! {
            format!("Pending{}", convert_date(" at ", scheduled_at.as_ref()))
        },
        grpc_client::execution_status::Status::BlockedByJoinSet(
            grpc_client::execution_status::BlockedByJoinSet { join_set_id, .. },
        ) => {
            let join_set_id = join_set_id.clone().expect("`join_set_id` is sent");
            format!("Blocked by {join_set_id}").to_html()
        }
        grpc_client::execution_status::Status::Finished(Finished { result_kind, .. }) => {
            let result_kind = result_kind
                .as_ref()
                .expect("ResultKind must be present for Finished status");
            match &result_kind.value {
                Some(grpc_client::result_kind::Value::Ok(_)) => html! {"Finished OK"},
                Some(grpc_client::result_kind::Value::Error(_)) => {
                    html! {"Finished with error"}
                }
                Some(grpc_client::result_kind::Value::ExecutionFailureKind(kind_i32)) => {
                    match grpc_client::ExecutionFailureKind::try_from(*kind_i32) {
                        Ok(kind) => match kind {
                            grpc_client::ExecutionFailureKind::TimedOut => {
                                html! {"Timeout"}
                            }
                            grpc_client::ExecutionFailureKind::NondeterminismDetected => {
                                html! { "Nondeterminism detected" }
                            }
                            grpc_client::ExecutionFailureKind::OutOfFuel => {
                                html! { "Out of fuel" }
                            }
                            grpc_client::ExecutionFailureKind::Cancelled => {
                                html! { "Cancelled" }
                            }
                            grpc_client::ExecutionFailureKind::Uncategorized => {
                                html! { "Execution failure" }
                            }
                            grpc_client::ExecutionFailureKind::Unspecified => {
                                html! { "Unspecified"}
                            }
                        },
                        Err(_) => {
                            html! { format!("Execution failure: Unknown variant ({})", kind_i32) }
                        }
                    }
                }
                None => html! {"Finished with unknown result"},
            }
        }
    }
}

fn convert_date(prefix: &str, date: Option<&::prost_wkt_types::Timestamp>) -> String {
    date.map(|date| {
        let date = DateTime::from(*date);
        format!("{prefix}{date:?}")
    })
    .unwrap_or_default()
}
