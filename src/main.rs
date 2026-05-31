mod activities;
mod app;
mod auth;
mod ui;

use activities::StravaClient;
use anyhow::Result;
use app::{App, Screen};
use auth::Config;
use chrono::{Local, TimeZone};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;
use tracing_appender::rolling;
use tracing_subscriber::{EnvFilter, fmt};

use crate::activities::{Coords, WeatherClient};

#[derive(Debug)]
enum AppMessage {
    ActivitiesLoaded(Vec<activities::Activity>),
    ActivitiesAppended(Vec<activities::Activity>),
    WeatherLoaded(),
    Error(String),
    StatusMsg(String),
}

fn init_logger() -> tracing_appender::non_blocking::WorkerGuard {
    // rotation journalière dans ./logs/app.log.YYYY-MM-DD
    let file_appender = rolling::daily("logs", "app.log");

    // évite les blocages (buffer + thread dédié)
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .with_ansi(false) // important pour fichier
        .init();

    guard
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = init_logger();

    let mut config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("❌ Configuration manquante: {}", e);
            eprintln!("\nCréez un fichier .env dans le répertoire courant avec :");
            eprintln!("  STRAVA_CLIENT_ID=votre_client_id");
            eprintln!("  STRAVA_CLIENT_SECRET=votre_client_secret");
            eprintln!("\nObtenez vos identifiants sur https://www.strava.com/settings/api");
            std::process::exit(1);
        }
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let app: Arc<Mutex<App>> = Arc::new(Mutex::new(App::new()));
    let (tx, mut rx) = mpsc::channel::<AppMessage>(32);

    // Auth (hors mode raw si besoin OAuth)
    let access_token = {
        // disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;

        let token_result = config.ensure_valid_token().await;

        // enable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture
        )?;
        terminal.clear()?;

        match token_result {
            Ok(t) => t,
            Err(e) => {
                // disable_raw_mode()?;
                execute!(
                    terminal.backend_mut(),
                    LeaveAlternateScreen,
                    DisableMouseCapture
                )?;
                eprintln!("❌ Erreur d'authentification: {}", e);
                std::process::exit(1);
            }
        }
    };

    // Charge la première page
    {
        let tx2 = tx.clone();
        let token = access_token.clone();
        {
            app.lock().unwrap().loading = true;
        }
        tokio::spawn(async move {
            let client = StravaClient::new(token);
            match client.get_activities(1, 50).await {
                Ok(acts) => {
                    let _ = tx2.send(AppMessage::ActivitiesLoaded(acts)).await;
                }
                Err(e) => {
                    let _ = tx2.send(AppMessage::Error(e.to_string())).await;
                }
            }
        });
    }

    loop {
        {
            let a = &app.lock().unwrap();
            terminal.draw(|f| ui::render(f, &a))?;
        }

        while let Ok(msg) = rx.try_recv() {
            let mut a = app.lock().unwrap();
            match msg {
                AppMessage::ActivitiesLoaded(acts) => {
                    a.loading = false;
                    let count = acts.len();
                    a.set_activities(acts);
                    a.status_msg = Some(format!("{} activités chargées", count));
                }
                AppMessage::ActivitiesAppended(acts) => {
                    a.loading = false;
                    let count = acts.len();
                    a.append_activities(acts);
                    if count == 0 {
                        a.status_msg = Some("Toutes les activités sont chargées".into());
                    } else {
                        a.status_msg =
                            Some(format!("{} activités supplémentaires chargées", count));
                    }
                }
                AppMessage::WeatherLoaded() => {
                    tracing::info!("weather");
                }
                AppMessage::Error(e) => {
                    a.loading = false;
                    a.error = Some(e);
                }
                AppMessage::StatusMsg(s) => {
                    a.status_msg = Some(s);
                }
            }
        }

        let tick_rate = Duration::from_millis(100);
        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                let mut a = app.lock().unwrap();
                a.status_msg = None;

                if a.error.is_some() {
                    a.error = None;
                    continue;
                }

                if a.search_mode {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter => {
                            a.search_mode = false;
                        }
                        KeyCode::Backspace => {
                            a.search_query.pop();
                            a.apply_filter_and_sort();
                        }
                        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                            a.search_query.push(c);
                            a.apply_filter_and_sort();
                        }
                        _ => {}
                    }
                    continue;
                }

                let visible_rows = terminal.size()?.height.saturating_sub(8) as usize;

                match (&a.screen.clone(), key.code) {
                    (_, KeyCode::Char('q')) => break,
                    (_, KeyCode::Char('c')) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break;
                    }
                    (_, KeyCode::Char('1')) => a.screen = Screen::List,
                    (_, KeyCode::Char('2')) => a.screen = Screen::Stats,
                    (_, KeyCode::Char('?') | KeyCode::Char('h')) => a.screen = Screen::Help,
                    (Screen::List, KeyCode::Up | KeyCode::Char('k')) => a.move_up(),
                    (Screen::List, KeyCode::Down | KeyCode::Char('j')) => a.move_down(visible_rows),
                    (Screen::List, KeyCode::PageUp) => {
                        for _ in 0..visible_rows {
                            a.move_up();
                        }
                    }
                    (Screen::List, KeyCode::PageDown) => {
                        for _ in 0..visible_rows {
                            a.move_down(visible_rows);
                        }
                    }
                    (Screen::List, KeyCode::Home | KeyCode::Char('g')) => {
                        a.selected = 0;
                        a.scroll_offset = 0;
                    }
                    (Screen::List, KeyCode::End | KeyCode::Char('G')) => {
                        a.selected = a.filtered.len().saturating_sub(1);
                        a.scroll_offset = a.selected.saturating_sub(visible_rows - 1);
                    }
                    (Screen::List, KeyCode::Enter) => {
                        if a.selected_activity().is_some() {
                            let coords = Coords {
                                lat: 43.493204146990244,
                                lng: 6.534099590754949,
                            };
                            let datetime = Local.with_ymd_and_hms(2026, 5, 31, 9, 30, 00).unwrap();

                            let weather_client = WeatherClient::new();
                            weather_client.get_weather(coords, datetime);
                            a.screen = Screen::Detail;
                        }
                    }
                    (Screen::Detail, KeyCode::Esc | KeyCode::Backspace) => {
                        a.screen = Screen::List;
                    }
                    (Screen::Detail, KeyCode::Up | KeyCode::Char('k')) => {
                        a.move_up();
                    }
                    (Screen::Detail, KeyCode::Down | KeyCode::Char('j')) => {
                        a.move_down(1);
                    }
                    (Screen::Help, KeyCode::Esc | KeyCode::Backspace) => {
                        a.screen = Screen::List;
                    }
                    (Screen::List, KeyCode::Char('f')) => {
                        a.toggle_filter();
                    }
                    (Screen::List, KeyCode::Char('s')) => {
                        a.toggle_sort();
                    }
                    (Screen::List, KeyCode::Char('r')) => {
                        a.toggle_sort_order();
                    }
                    (Screen::List, KeyCode::Char('/')) => {
                        a.search_mode = true;
                    }
                    (Screen::List, KeyCode::Esc) => {
                        if !a.search_query.is_empty() {
                            a.search_query.clear();
                            a.apply_filter_and_sort();
                        }
                    }
                    (_, KeyCode::Tab) => {
                        load_next_page(&mut a, tx.clone(), access_token.clone());
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    println!("À bientôt ! 🏃");
    Ok(())
}

fn load_next_page(app: &mut App, tx: mpsc::Sender<AppMessage>, access_token: String) {
    if app.loading {
        return;
    }

    app.loading = true;
    app.page += 1;

    let page = app.page;

    tokio::spawn(async move {
        let client = StravaClient::new(access_token);

        match client.get_activities(page, 50).await {
            Ok(acts) => {
                let _ = tx.send(AppMessage::ActivitiesAppended(acts)).await;
            }
            Err(e) => {
                let _ = tx.send(AppMessage::Error(e.to_string())).await;
            }
        }
    });
}
