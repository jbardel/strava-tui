use std::sync::{Arc, Mutex};

use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::{AppMessage, app::App};

struct EventManager {
    tx: Sender<AppMessage>,
    app: Arc<Mutex<App>>,
}

impl EventManager {
    pub fn create(app: Arc<Mutex<App>>) -> Self {
        let (tx, rx) = mpsc::channel::<AppMessage>(32);
        let event_manager = EventManager { tx: tx, app: app };
        event_manager.init_receiver(rx);
        return event_manager;
    }

    fn init_receiver(&self, mut rx: Receiver<AppMessage>) {
        while let Ok(msg) = rx.try_recv() {
            let mut a = self.app.lock().unwrap();
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
    }

    pub fn get_tx(self) -> Sender<AppMessage> {
        return self.tx.clone();
    }
}

#[cfg(test)]
mod tests {

    use std::sync::{Arc, Mutex};

    use crate::{AppMessage::WeatherLoaded, app::App, event_management::EventManager};

    #[tokio::test]
    async fn test_event_management() {
        let app = Arc::new(Mutex::new(App::new()));
        let event_manager = EventManager::create(app);
        let tx = event_manager.get_tx();
        let _ = tx.send(WeatherLoaded()).await;

    }
}
