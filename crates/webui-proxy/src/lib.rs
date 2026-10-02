use wstd::http::{Body, Client, Error, HeaderValue, Request, Response, Uri};

#[wstd::http_server]
async fn main(server_req: Request<Body>) -> Result<Response<Body>, Error> {
    match server_req.uri().path_and_query().unwrap().as_str() {
        path if let Some((content, content_type)) = static_asset(path) => {
            write_static_response(content, content_type)
        }
        api_prefixed_path if api_prefixed_path.starts_with("/api") => {
            // Remove /api prefix
            let target_url =
                std::env::var("TARGET_URL").expect("missing environment variable TARGET_URL");
            let target_url: Uri = format!(
                "{target_url}{}",
                api_prefixed_path
                    .strip_prefix("/api")
                    .expect("checked above")
            )
            .parse()
            .expect("final target url should be parseable");
            proxy(server_req, target_url).await
        }
        _ => write_static_response(INDEX, "text/html"),
    }
}

async fn proxy(server_req: Request<Body>, target_url: Uri) -> Result<Response<Body>, Error> {
    let client = Client::new();
    let mut client_req = Request::builder();
    client_req = client_req.uri(target_url).method(server_req.method());

    // Copy headers from `server_req` to the `client_req`.
    for (key, value) in server_req.headers() {
        client_req = client_req.header(key, value);
    }

    // Stream the request body.
    let client_req = client_req.body(server_req.into_body())?;
    // Send the request.
    let client_resp = client.send(client_req).await?;
    // Copy headers from `client_resp` to `server_resp`.
    let mut server_resp = Response::builder().status(client_resp.status());
    for (key, value) in client_resp.headers() {
        server_resp
            .headers_mut()
            .expect("no errors could be in ResponseBuilder")
            .append(key, value.clone());
    }
    Ok(server_resp.body(client_resp.into_body())?)
}

fn write_static_response(body: &[u8], content_type: &'static str) -> Result<Response<Body>, Error> {
    let mut resp = Response::builder();
    resp.headers_mut()
        .unwrap()
        .append("content-type", HeaderValue::from_static(content_type));
    resp.body(body.into()).map_err(Error::from)
}

// release: Include real files
#[cfg(not(debug_assertions))]
macro_rules! dist {
    ($file:literal) => {
        include_bytes!(concat!("../../webui/dist/", $file))
    };
}

// debug: Include dummy file content
#[cfg(debug_assertions)]
macro_rules! dist {
    ($file:literal) => {
        &[]
    };
}

const INDEX: &[u8] = dist!("index.html");

fn static_asset(path: &str) -> Option<(&'static [u8], &'static str)> {
    Some(match path {
        "/webui_bg.wasm" => (dist!("webui_bg.wasm"), "application/wasm"),
        "/webui.js" => (dist!("webui.js"), "text/javascript"),
        "/styles.css" => (dist!("styles.css"), "text/css"),
        "/syntect.css" => (dist!("syntect.css"), "text/css"),
        "/syntect-light.css" => (dist!("syntect-light.css"), "text/css"),
        "/logo.png" => (dist!("logo.png"), "image/png"),
        _ => return None,
    })
}
