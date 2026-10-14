use log::error;
use webui::{
    app::{App, AppProps},
    loader::{get_server_info, load_components},
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
        let server_info = get_server_info()
            .await
            .inspect_err(|e| error!("Failed to fetch server info: {e}"))
            .ok();
        let deployment_id = server_info.as_ref().map(|info| info.deployment_id);
        let app_name = server_info.map(|info| info.app_name);
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
