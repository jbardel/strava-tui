use anyhow::{Result, anyhow};
use chrono::{DateTime, Local, Utc};
use hyper::{Body, Client, Request};
use hyper_rustls::HttpsConnectorBuilder;
use serde::{Deserialize, Serialize};

pub type HttpsClient = Client<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>;

pub fn build_client() -> HttpsClient {
    let https = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .build();
    Client::builder().build(https)
}

async fn http_get_json<T: serde::de::DeserializeOwned>(
    client: &HttpsClient,
    url: &str,
    bearer: &str,
) -> Result<T> {
    let req = Request::builder()
        .uri(url)
        .header("Authorization", format!("Bearer {}", bearer))
        .header("User-Agent", "strava-tui/0.1")
        .body(Body::empty())?;

    let resp = client.request(req).await?;
    let status = resp.status();
    let bytes = hyper::body::to_bytes(resp.into_body()).await?;
    if !status.is_success() {
        let body = String::from_utf8_lossy(&bytes);
        return Err(anyhow!("Erreur appel URL {}: {}", status, body));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub async fn post_form_json<T: serde::de::DeserializeOwned>(
    client: &HttpsClient,
    url: &str,
    form_body: String,
) -> Result<T> {
    let req = Request::builder()
        .method("POST")
        .uri(url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("User-Agent", "strava-tui/0.1")
        .body(Body::from(form_body))?;

    let resp = client.request(req).await?;
    let status = resp.status();
    let bytes = hyper::body::to_bytes(resp.into_body()).await?;
    if !status.is_success() {
        let body = String::from_utf8_lossy(&bytes);
        return Err(anyhow!("Erreur POST {}: {}", status, body));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

/// Token OAuth2 Strava
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
    pub athlete: Option<Athlete>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Athlete {
    pub id: u64,
    pub firstname: String,
    pub lastname: String,
}

/// Activité Strava
#[derive(Debug, Deserialize, Clone)]
pub struct Activity {
    pub id: u64,
    pub name: String,
    #[serde(rename = "type")]
    pub activity_type: String,
    pub start_date: DateTime<Utc>,
    pub distance: f64,
    pub moving_time: u64,
    pub total_elevation_gain: f64,
    pub average_speed: f64,
    pub max_speed: f64,
    pub average_heartrate: Option<f64>,
    pub max_heartrate: Option<f64>,
    pub suffer_score: Option<f64>,
    pub kudos_count: u32,
    pub achievement_count: u32,
}

impl Activity {
    pub fn distance_km(&self) -> f64 {
        self.distance / 1000.0
    }

    pub fn duration_formatted(&self) -> String {
        let h = self.moving_time / 3600;
        let m = (self.moving_time % 3600) / 60;
        let s = self.moving_time % 60;
        if h > 0 {
            format!("{}h{:02}m{:02}s", h, m, s)
        } else {
            format!("{}m{:02}s", m, s)
        }
    }

    pub fn pace_per_km(&self) -> String {
        if self.average_speed < 0.01 {
            return "-".to_string();
        }
        let secs_per_km = 1000.0 / self.average_speed;
        let m = secs_per_km as u64 / 60;
        let s = secs_per_km as u64 % 60;
        format!("{}'{:02}\"", m, s)
    }

    pub fn date_short(&self) -> String {
        self.start_date.format("%d/%m/%Y").to_string()
    }

    pub fn is_run(&self) -> bool {
        matches!(
            self.activity_type.as_str(),
            "Run" | "TrailRun" | "VirtualRun"
        )
    }

    pub fn type_icon(&self) -> &str {
        match self.activity_type.as_str() {
            "Run" => "🏃",
            "TrailRun" => "🏔",
            "VirtualRun" => "💻",
            "Ride" => "🚴",
            "Swim" => "🏊",
            "Walk" => "🚶",
            "Hike" => "🥾",
            _ => "⚡",
        }
    }
}

pub struct StravaClient {
    pub access_token: String,
    client: HttpsClient,
}

impl StravaClient {
    pub fn new(access_token: String) -> Self {
        Self {
            access_token,
            client: build_client(),
        }
    }

    pub async fn get_activities(&self, page: u32, per_page: u32) -> Result<Vec<Activity>> {
        let url = format!(
            "https://www.strava.com/api/v3/athlete/activities?page={}&per_page={}",
            page, per_page
        );
        let activities = http_get_json(&self.client, &url, &self.access_token).await;

        return activities;
    }

    pub async fn refresh_token(
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
    ) -> Result<TokenResponse> {
        let client = build_client();
        let body = format!(
            "client_id={}&client_secret={}&refresh_token={}&grant_type=refresh_token",
            url_encode(client_id),
            url_encode(client_secret),
            url_encode(refresh_token)
        );
        post_form_json(&client, "https://www.strava.com/oauth/token", body).await
    }
}

fn url_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

#[derive(Debug, Deserialize, Clone)]
pub struct WeatherResult {
    elevation: f32,
    latitude: f32,
    longitude: f32,
    hourly_units: Units,
    hourly: TemperatureData,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TemperatureData {
    weather_code: Vec<u8>,
    temperature_2m: Vec<f32>,
    time: Vec<String>,
    wind_speed_10m: Vec<f32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Units {
    time: String,
    weather_code: String,
    temperature_2m: String,
    wind_speed_10m: String,
}

pub struct WeatherClient {
    client: HttpsClient,
}

pub struct Coords {
    pub lat: f32,
    pub lng: f32,
}

impl WeatherClient {
    pub fn new() -> Self {
        Self {
            client: build_client(),
        }
    }

    pub async fn get_weather(
        &self,
        coords: Coords,
        datetime: DateTime<Local>,
    ) -> Result<WeatherResult> {
        let url = format!(
            "https://archive-api.open-meteo.com/v1/archive?latitude={lat}&longitude={lng}&start_date={date}&end_date={date}&hourly=weather_code,temperature_2m,wind_speed_10m",
            lat = coords.lat,
            lng = coords.lng,
            date = datetime
        );

        http_get_json(&self.client, &url, "").await
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Local, TimeZone};

    use crate::activities::{Coords, WeatherClient};

    #[tokio::test]
    async fn test_weather() {
        let weather_client = WeatherClient::new();

        // 43.493204146990244, 6.534099590754949
        let coords = Coords {
            lat: 43.493204146990244,
            lng: 6.534099590754949,
        };

        let datetime = Local.with_ymd_and_hms(2026, 5, 31, 9, 30, 00).unwrap();
        let val = weather_client
            .get_weather(coords, datetime)
            .await
            .expect("get_current_weather failed");
        println!("{:?}", val);
    }
}
