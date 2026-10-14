use log::error;
use webui::{
    app::{App, AppProps},
    loader::{get_app_name, get_current_deployment_id, load_components},
};

fn main() {
    init_logging();
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&format!("RUST PANIC: {info}").into());
    }));

    wasm_bindgen_futures::spawn_local(async move {
        let loaded = match load_components().await {
            Ok(loaded) => Some(loaded),
            Err(e) => {
                error!("Failed to reload components: {e}");
                None
            }
        }
        .unwrap_or_default();
        let deployment_id = get_current_deployment_id().await.ok();
        let app_name = get_app_name()
            .await
            .inspect_err(|e| error!("Failed to fetch app name: {e}"))
            .ok();
        if let Some(app_name) = &app_name {
            gloo::utils::document().set_title(&format!("{app_name} | Obelisk WebUI"));
        }

        yew::Renderer::<App>::with_props(AppProps {
            initial_components: loaded,
            initial_deployment_id: deployment_id,
            app_name,
        })
        .render();
    });
}

fn init_logging() {
    use log::Level;
    use wasm_logger::Config;

    // use debug level for debug builds, warn level for production builds.
    #[cfg(debug_assertions)]
    let level = Level::Trace;
    #[cfg(not(debug_assertions))]
    let level = Level::Warn;

    wasm_logger::init(Config::new(level));
}
