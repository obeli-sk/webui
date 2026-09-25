use std::cell::RefCell;
use web_sys::{HtmlInputElement, SubmitEvent};
use yew::prelude::*;

const TOKEN_STORAGE_KEY: &str = "obelisk-api-token";

thread_local! {
    static ON_AUTH_REQUIRED: RefCell<Option<Callback<()>>> = const { RefCell::new(None) };
    static AUTH_REQUIRED_PENDING: RefCell<bool> = const { RefCell::new(false) };
}

pub(crate) fn token() -> Option<String> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(TOKEN_STORAGE_KEY)
        .ok()?
}

pub(crate) fn auth_required() {
    AUTH_REQUIRED_PENDING.with(|pending| *pending.borrow_mut() = true);
    ON_AUTH_REQUIRED.with(|callback| {
        if let Some(callback) = callback.borrow().as_ref() {
            callback.emit(());
        }
    });
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
