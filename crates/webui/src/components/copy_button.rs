use gloo::timers::callback::Timeout;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct CopyButtonProps {
    /// Text copied to the clipboard on click.
    pub text: String,
}

/// Button that copies `text` to the clipboard and briefly confirms.
#[component(CopyButton)]
pub fn copy_button(CopyButtonProps { text }: &CopyButtonProps) -> Html {
    let copied = use_state(|| false);

    let onclick = {
        let text = text.clone();
        let copied = copied.clone();
        Callback::from(move |_| {
            let clipboard = web_sys::window()
                .expect("window should exist")
                .navigator()
                .clipboard();
            let promise = clipboard.write_text(&text);
            let copied = copied.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if wasm_bindgen_futures::JsFuture::from(promise).await.is_ok() {
                    copied.set(true);
                    let copied = copied.clone();
                    Timeout::new(1500, move || copied.set(false)).forget();
                }
            });
        })
    };

    html! {
        <button class="action-button copy-button" {onclick}>
            { if *copied { "Copied!" } else { "Copy" } }
        </button>
    }
}
