use crate::strava::{build_client, post_form_json, TokenResponse};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub client_id: String,
    pub client_secret: String,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

impl Config {
    pub fn config_path() -> std::path::PathBuf {
        let home = dirs_next::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        home.join(".strava-tui").join("config.json")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Err(anyhow!("Pas de configuration trouvée"));
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();
        let client_id = std::env::var("STRAVA_CLIENT_ID")
            .map_err(|_| anyhow!("STRAVA_CLIENT_ID non défini"))?;
        let client_secret = std::env::var("STRAVA_CLIENT_SECRET")
            .map_err(|_| anyhow!("STRAVA_CLIENT_SECRET non défini"))?;

        if let Ok(mut saved) = Self::load() {
            saved.client_id = client_id;
            saved.client_secret = client_secret;
            return Ok(saved);
        }
        Ok(Self {
            client_id,
            client_secret,
            access_token: None,
            refresh_token: None,
            expires_at: None,
        })
    }

    pub fn is_token_valid(&self) -> bool {
        if let (Some(_), Some(exp)) = (&self.access_token, &self.expires_at) {
            return *exp > chrono::Utc::now().timestamp() + 60;
        }
        false
    }

    pub async fn ensure_valid_token(&mut self) -> Result<String> {
        if self.is_token_valid() {
            return Ok(self.access_token.clone().unwrap());
        }
        if let Some(rt) = self.refresh_token.clone() {
            eprintln!("Rafraîchissement du token Strava...");
            let token = crate::strava::StravaClient::refresh_token(
                &self.client_id,
                &self.client_secret,
                &rt,
            )
            .await?;
            self.apply_token(&token);
            self.save()?;
            return Ok(self.access_token.clone().unwrap());
        }
        let token = self.oauth_flow().await?;
        self.apply_token(&token);
        self.save()?;
        Ok(self.access_token.clone().unwrap())
    }

    fn apply_token(&mut self, token: &TokenResponse) {
        self.access_token = Some(token.access_token.clone());
        self.refresh_token = Some(token.refresh_token.clone());
        self.expires_at = Some(token.expires_at);
    }

    async fn oauth_flow(&self) -> Result<TokenResponse> {
        let auth_url = format!(
            "https://www.strava.com/oauth/authorize?client_id={}&response_type=code&redirect_uri=http://localhost:8765/callback&approval_prompt=force&scope=read,activity:read_all",
            self.client_id
        );

        println!("\n╔══════════════════════════════════════════════════╗");
        println!("║         Authentification Strava requise          ║");
        println!("╚══════════════════════════════════════════════════╝");
        println!("\nOuverture du navigateur...");
        println!("Lien manuel si nécessaire :\n{}\n", auth_url);

        let _ = open::that(&auth_url);
        let code = wait_for_oauth_code(8765)?;

        let client = build_client();
        let body = format!(
            "client_id={}&client_secret={}&code={}&grant_type=authorization_code",
            url_encode(&self.client_id),
            url_encode(&self.client_secret),
            url_encode(&code)
        );
        let token: TokenResponse =
            post_form_json(&client, "https://www.strava.com/oauth/token", body).await?;

        if let Some(athlete) = &token.athlete {
            println!(
                "\n✅ Connecté en tant que {} {}\n",
                athlete.firstname, athlete.lastname
            );
        }
        Ok(token)
    }
}

fn wait_for_oauth_code(port: u16) -> Result<String> {
    let server = tiny_http::Server::http(format!("127.0.0.1:{}", port))
        .map_err(|e| anyhow!("Serveur OAuth impossible: {}", e))?;

    println!(
        "En attente du callback OAuth sur http://localhost:{}/callback ...",
        port
    );

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        if let Some(code) = parse_code_from_url(&url) {
            let html = "<html><body><h2>✅ Authentification réussie ! Vous pouvez fermer cet onglet.</h2></body></html>";
            let response = tiny_http::Response::from_string(html).with_header(
                "Content-Type: text/html; charset=utf-8"
                    .parse::<tiny_http::Header>()
                    .unwrap(),
            );
            let _ = request.respond(response);
            return Ok(code);
        }
        let _ = request.respond(tiny_http::Response::from_string("Erreur: code manquant"));
    }
    Err(anyhow!("Serveur OAuth fermé sans code"))
}

fn parse_code_from_url(url: &str) -> Option<String> {
    let query = url.split('?').nth(1)?;
    for param in query.split('&') {
        let mut parts = param.splitn(2, '=');
        if let (Some("code"), Some(val)) = (parts.next(), parts.next()) {
            return Some(val.to_string());
        }
    }
    None
}

fn url_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}
