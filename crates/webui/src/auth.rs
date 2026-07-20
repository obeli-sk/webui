use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tonic::{
    body::Body,
    codegen::{Service, http},
};
use tonic_web_wasm_client::{Client, Error, ResponseBody};
use web_sys::{HtmlInputElement, SubmitEvent};
use yew::prelude::*;

use crate::BASE_URL;

const TOKEN_STORAGE_KEY: &str = "obelisk-api-token";

thread_local! {
    static ON_AUTH_REQUIRED: RefCell<Option<Callback<()>>> = const { RefCell::new(None) };
    static AUTH_REQUIRED_PENDING: RefCell<bool> = const { RefCell::new(false) };
}

fn token() -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(TOKEN_STORAGE_KEY)
        .ok()?
}

fn auth_required() {
    AUTH_REQUIRED_PENDING.with(|pending| *pending.borrow_mut() = true);
    ON_AUTH_REQUIRED.with(|callback| {
        if let Some(callback) = callback.borrow().as_ref() {
            callback.emit(());
        }
    });
}

#[derive(Clone)]
pub struct AuthenticatedClient(Client);

pub fn client() -> AuthenticatedClient {
    AuthenticatedClient(Client::new(BASE_URL.to_string()))
}

impl Service<http::Request<Body>> for AuthenticatedClient {
    type Response = http::Response<ResponseBody>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx)
    }

    fn call(&mut self, mut request: http::Request<Body>) -> Self::Future {
        if let Some(token) = token()
            && let Ok(mut value) = format!("Bearer {token}").parse::<http::HeaderValue>()
        {
            value.set_sensitive(true);
            request
                .headers_mut()
                .insert(http::header::AUTHORIZATION, value);
        }

        let future = self.0.call(request);
        Box::pin(async move {
            let response = future.await?;
            if response
                .headers()
                .get("grpc-status")
                .is_some_and(|status| status == "16")
            {
                auth_required();
            }
            Ok(response)
        })
    }
}

#[derive(Properties, PartialEq)]
pub struct AuthProviderProps {
    pub children: Children,
}

#[component(AuthProvider)]
pub fn auth_provider(props: &AuthProviderProps) -> Html {
    let show_dialog = use_state(|| false);
    let token_input = use_node_ref();

    {
        let show_dialog = show_dialog.clone();
        use_effect_with((), move |()| {
            ON_AUTH_REQUIRED.with(|callback| {
                *callback.borrow_mut() = Some(Callback::from({
                    let show_dialog = show_dialog.clone();
                    move |()| show_dialog.set(true)
                }));
            });
            AUTH_REQUIRED_PENDING.with(|pending| {
                if *pending.borrow() {
                    show_dialog.set(true);
                }
            });
            || ON_AUTH_REQUIRED.with(|callback| *callback.borrow_mut() = None)
        });
    }

    let onsubmit = {
        let token_input = token_input.clone();
        Callback::from(move |event: SubmitEvent| {
            event.prevent_default();
            let Some(input) = token_input.cast::<HtmlInputElement>() else {
                return;
            };
            let token = input.value();
            let token = token.trim().strip_prefix("Bearer ").unwrap_or(token.trim());
            if token.is_empty() {
                return;
            }
            if let Some(storage) =
                web_sys::window().and_then(|window| window.local_storage().ok().flatten())
            {
                let _ = storage.set_item(TOKEN_STORAGE_KEY, token);
            }
            AUTH_REQUIRED_PENDING.with(|pending| *pending.borrow_mut() = false);
            if let Some(window) = web_sys::window() {
                let _ = window.location().reload();
            }
        })
    };

    html! {
        <>
            {props.children.clone()}
            if *show_dialog {
                <div class="modal-overlay auth-modal-overlay" role="presentation">
                    <section
                        class="modal-window auth-modal-window"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="auth-modal-title"
                    >
                        <form onsubmit={onsubmit}>
                            <div class="modal-header">
                                <h3 id="auth-modal-title">{"Authentication required"}</h3>
                            </div>
                            <div class="auth-modal-body">
                                <p>{"Paste an Obelisk API token to continue. It will be kept in this browser."}</p>
                                <label for="auth-token">{"API token"}</label>
                                <input
                                    ref={token_input}
                                    id="auth-token"
                                    name="auth-token"
                                    type="password"
                                    autocomplete="off"
                                    autofocus=true
                                    required=true
                                />
                            </div>
                            <div class="modal-footer">
                                <button type="submit" class="action-button confirm">{"Continue"}</button>
                            </div>
                        </form>
                    </section>
                </div>
            }
        </>
    }
}
