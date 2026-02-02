use webui::{
    app::{App, AppProps},
    loader::load_components,
};

fn main() {
    init_logging();
    wasm_bindgen_futures::spawn_local(async move {
        let loaded = load_components().await.unwrap();

        yew::Renderer::<App>::with_props(AppProps {
            initial_components: loaded,
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
