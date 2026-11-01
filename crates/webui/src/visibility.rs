use futures::channel::oneshot;
use gloo::events::EventListener;
use std::{cell::RefCell, rc::Rc};
use yew::prelude::*;

pub fn document_visible() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_none_or(|document| !document.hidden())
}

pub async fn wait_until_visible() {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    while document.hidden() {
        let (sender, receiver) = oneshot::channel();
        let sender = Rc::new(RefCell::new(Some(sender)));
        let watched = document.clone();
        let listener = EventListener::new(&document, "visibilitychange", move |_| {
            if !watched.hidden()
                && let Some(sender) = sender.borrow_mut().take()
            {
                let _ = sender.send(());
            }
        });
        if document.hidden() {
            let _ = receiver.await;
        }
        drop(listener);
    }
}

#[hook]
pub fn use_document_visible() -> bool {
    let visible = use_state(document_visible);
    {
        let visible = visible.clone();
        use_effect_with((), move |()| {
            let listener = web_sys::window()
                .and_then(|window| window.document())
                .map(|document| {
                    let visible = visible.clone();
                    EventListener::new(&document, "visibilitychange", move |_| {
                        visible.set(document_visible())
                    })
                });
            visible.set(document_visible());
            move || drop(listener)
        });
    }
    *visible
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use gloo::timers::future::TimeoutFuture;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen(inline_js = r"
        let originalFetch;
        let calls;
        let aborts;
        let pending;
        export function setup() {
            originalFetch = window.fetch;
            calls = 0;
            aborts = 0;
            pending = false;
            window.fetch = (request) => {
                calls++;
                return new Promise((resolve, reject) => {
                    request.signal.addEventListener('abort', () => {
                        aborts++;
                        reject(new DOMException('aborted', 'AbortError'));
                    });
                    if (!pending) resolve(new Response(new ReadableStream({
                        start(controller) { controller.enqueue(new TextEncoder().encode(': heartbeat\n\n')); }
                    }), { headers: { 'content-type': 'text/event-stream' } }));
                });
            };
        }
        export function hidden(value) {
            Object.defineProperty(document, 'hidden', { configurable: true, value });
            document.dispatchEvent(new Event('visibilitychange'));
        }
        export function pendingHeaders() { pending = true; }
        export function requests() { return calls; }
        export function cancelled() { return aborts; }
        export function cleanup() { window.fetch = originalFetch; delete document.hidden; }
    ")]
    extern "C" {
        fn setup();
        fn hidden(value: bool);
        #[wasm_bindgen(js_name = pendingHeaders)]
        fn pending_headers();
        fn requests() -> u32;
        fn cancelled() -> u32;
        fn cleanup();
    }

    #[wasm_bindgen_test(async)]
    async fn streams_wait_for_visibility_and_abort_even_before_headers() {
        setup();
        hidden(true);
        let hub = crate::status_hub::StatusHub::default();
        hub.set_visible(true);
        let watch = hub.watch(&"E_01KABCDEFGHJKMNPQRSTVWXYZ0".parse().unwrap());
        TimeoutFuture::new(50).await;
        assert_eq!(requests(), 0);
        hidden(false);
        TimeoutFuture::new(50).await;
        assert_eq!(requests(), 1);
        hidden(true);
        hub.set_visible(false);
        TimeoutFuture::new(10).await;
        assert_eq!(cancelled(), 1);
        hub.set_visible(true);
        TimeoutFuture::new(50).await;
        assert_eq!(requests(), 1);
        hidden(false);
        TimeoutFuture::new(50).await;
        assert_eq!(requests(), 2);
        hub.set_visible(false);
        TimeoutFuture::new(10).await;
        assert_eq!(cancelled(), 2);
        pending_headers();
        hub.set_visible(true);
        TimeoutFuture::new(50).await;
        assert_eq!(requests(), 3);
        hub.set_visible(false);
        TimeoutFuture::new(10).await;
        assert_eq!(cancelled(), 3);
        drop(watch);
        cleanup();
    }
}
