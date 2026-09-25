use gloo::net::http::Request;
use serde::{Serialize, de::DeserializeOwned};

pub async fn get<T: DeserializeOwned>(path: &str, query: &[(&str, String)]) -> Result<T, String> {
    let mut request = Request::get(&format!("{}{}", crate::BASE_URL, path))
        .header("Accept", "application/json")
        .query(query.iter().map(|(key, value)| (*key, value.as_str())));
    if let Some(token) = crate::auth::token() {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    let response = request.send().await.map_err(|error| error.to_string())?;
    decode(response).await
}

pub async fn put<B: Serialize + ?Sized, T: DeserializeOwned>(
    path: &str,
    body: &B,
) -> Result<T, String> {
    let mut request =
        Request::put(&format!("{}{}", crate::BASE_URL, path)).header("Accept", "application/json");
    if let Some(token) = crate::auth::token() {
        request = request.header("Authorization", &format!("Bearer {token}"));
    }
    let response = request
        .json(body)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;
    decode(response).await
}

async fn decode<T: DeserializeOwned>(response: gloo::net::http::Response) -> Result<T, String> {
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
