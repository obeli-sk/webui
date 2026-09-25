use gloo::net::http::Request;
use serde::de::DeserializeOwned;

pub async fn get<T: DeserializeOwned>(path: &str, query: &[(&str, String)]) -> Result<T, String> {
    let mut request = Request::get(&format!("{}{}", crate::BASE_URL, path))
        .header("Accept", "application/json")
        .query(query.iter().map(|(key, value)| (*key, value.as_str())));
    if let Some(token) = crate::auth::token() {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    let response = request.send().await.map_err(|error| error.to_string())?;
    if response.status() == 401 {
        crate::auth::auth_required();
    }
    if !response.ok() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }
    response.json().await.map_err(|error| error.to_string())
}
