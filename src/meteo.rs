use anyhow::{anyhow, Result};
use hyper::{body::to_bytes, Body, Request};

use crate::strava::HttpsClient;

pub async fn get_current_weather(client: &HttpsClient) -> Result<serde_json::Value> {
    println!("get_current_weather");

    let req = Request::builder()
    .method("GET")
    .uri("https://archive-api.open-meteo.com/v1/archive?latitude=43.49&longitude=6.36&start_date=2026-05-15&end_date=2026-05-15&hourly=temperature_2m")
    .header("User-Agent", "strava-tui/0.1")
    .body(Body::empty())?;

    let resp = client.request(req).await?;
    let status = resp.status();
    let bytes = to_bytes(resp.into_body()).await?;
    if !status.is_success() {
        let body = String::from_utf8_lossy(&bytes);
        return Err(anyhow!("Erreur API Strava {}: {}", status, body));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use crate::meteo::get_current_weather;
    use crate::strava::build_client;

    #[tokio::test]
    async fn test_weather() {
        let client = build_client();
        let val = get_current_weather(&client)
            .await
            .expect("get_current_weather failed");
        assert!(val.is_object());
        println!("{}", val);
    }
}
