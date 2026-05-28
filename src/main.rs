mod app;
mod auth;
mod strava;
mod ui;

use anyhow::Result;
use app::{App, Screen};
use auth::Config;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};
use strava::StravaClient;
use tokio::sync::mpsc;

#[derive(Debug)]
enum AppMessage {
    ActivitiesLoaded(Vec<strava::Activity>),
    ActivitiesAppended(Vec<strava::Activity>),
    Error(String),
    StatusMsg(String),
}

#[tokio::main]
async fn main() -> Result<()> {
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

    let app = Arc::new(Mutex::new(App::new()));
    let (tx, mut rx) = mpsc::channel::<AppMessage>(32);

    // Auth (hors mode raw si besoin OAuth)
    let access_token = {
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;

        let token_result = config.ensure_valid_token().await;

        enable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture
        )?;
        terminal.clear()?;

        match token_result {
            Ok(t) => t,
            Err(e) => {
                disable_raw_mode()?;
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

    let tick_rate = Duration::from_millis(100);

    loop {
        {
            let a = app.lock().unwrap();
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
                AppMessage::Error(e) => {
                    a.loading = false;
                    a.error = Some(e);
                }
                AppMessage::StatusMsg(s) => {
                    a.status_msg = Some(s);
                }
            }
        }

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
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
                        break
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
                        if !a.loading {
                            a.loading = true;
                            a.page += 1;
                            let tx2 = tx.clone();
                            let token = access_token.clone();
                            let page = a.page;
                            tokio::spawn(async move {
                                let client = StravaClient::new(token);
                                match client.get_activities(page, 50).await {
                                    Ok(acts) => {
                                        let _ =
                                            tx2.send(AppMessage::ActivitiesAppended(acts)).await;
                                    }
                                    Err(e) => {
                                        let _ = tx2.send(AppMessage::Error(e.to_string())).await;
                                    }
                                }
                            });
                        }
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
