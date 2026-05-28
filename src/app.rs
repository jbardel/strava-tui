use crate::strava::Activity;

#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    List,
    Detail,
    Stats,
    Help,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    All,
    RunsOnly,
    ByType(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SortBy {
    Date,
    Distance,
    Duration,
    Pace,
}

pub struct App {
    pub activities: Vec<Activity>,
    pub filtered: Vec<usize>, // indices dans activities
    pub selected: usize,
    pub scroll_offset: usize,
    pub screen: Screen,
    pub filter: Filter,
    pub sort_by: SortBy,
    pub sort_desc: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub status_msg: Option<String>,
    pub page: u32,
    pub total_loaded: usize,
    pub search_query: String,
    pub search_mode: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            activities: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            screen: Screen::List,
            filter: Filter::All,
            sort_by: SortBy::Date,
            sort_desc: true,
            loading: false,
            error: None,
            status_msg: None,
            page: 1,
            total_loaded: 0,
            search_query: String::new(),
            search_mode: false,
        }
    }

    pub fn set_activities(&mut self, activities: Vec<Activity>) {
        self.activities = activities;
        self.apply_filter_and_sort();
    }

    pub fn append_activities(&mut self, new: Vec<Activity>) {
        self.activities.extend(new);
        self.apply_filter_and_sort();
    }

    pub fn apply_filter_and_sort(&mut self) {
        // Filtre
        let q = self.search_query.to_lowercase();
        self.filtered = self
            .activities
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                let type_match = match &self.filter {
                    Filter::All => true,
                    Filter::RunsOnly => a.is_run(),
                    Filter::ByType(t) => &a.activity_type == t,
                };
                let search_match = if q.is_empty() {
                    true
                } else {
                    a.name.to_lowercase().contains(&q)
                        || a.activity_type.to_lowercase().contains(&q)
                        || a.date_short().contains(&q)
                };
                type_match && search_match
            })
            .map(|(i, _)| i)
            .collect();

        // Tri
        let activities = &self.activities;
        let sort_by = &self.sort_by;
        let desc = self.sort_desc;
        self.filtered.sort_by(|&a, &b| {
            let ord = match sort_by {
                SortBy::Date => activities[a].start_date.cmp(&activities[b].start_date),
                SortBy::Distance => activities[a]
                    .distance
                    .partial_cmp(&activities[b].distance)
                    .unwrap_or(std::cmp::Ordering::Equal),
                SortBy::Duration => activities[a].moving_time.cmp(&activities[b].moving_time),
                SortBy::Pace => activities[a]
                    .average_speed
                    .partial_cmp(&activities[b].average_speed)
                    .unwrap_or(std::cmp::Ordering::Equal),
            };
            if desc {
                ord.reverse()
            } else {
                ord
            }
        });

        // Recentre la sélection
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    pub fn selected_activity(&self) -> Option<&Activity> {
        let idx = self.filtered.get(self.selected)?;
        self.activities.get(*idx)
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            if self.selected < self.scroll_offset {
                self.scroll_offset = self.selected;
            }
        }
    }

    pub fn move_down(&mut self, visible: usize) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
            if self.selected >= self.scroll_offset + visible {
                self.scroll_offset = self.selected - visible + 1;
            }
        }
    }

    pub fn toggle_filter(&mut self) {
        self.filter = match &self.filter {
            Filter::All => Filter::RunsOnly,
            Filter::RunsOnly => Filter::All,
            Filter::ByType(_) => Filter::All,
        };
        self.apply_filter_and_sort();
    }

    pub fn toggle_sort(&mut self) {
        self.sort_by = match self.sort_by {
            SortBy::Date => SortBy::Distance,
            SortBy::Distance => SortBy::Duration,
            SortBy::Duration => SortBy::Pace,
            SortBy::Pace => SortBy::Date,
        };
        self.apply_filter_and_sort();
    }

    pub fn toggle_sort_order(&mut self) {
        self.sort_desc = !self.sort_desc;
        self.apply_filter_and_sort();
    }

    pub fn filter_label(&self) -> &str {
        match &self.filter {
            Filter::All => "Toutes",
            Filter::RunsOnly => "Courses",
            Filter::ByType(t) => t.as_str(),
        }
    }

    pub fn sort_label(&self) -> &str {
        match self.sort_by {
            SortBy::Date => "Date",
            SortBy::Distance => "Distance",
            SortBy::Duration => "Durée",
            SortBy::Pace => "Allure",
        }
    }

    /// Statistiques globales sur les activités filtrées
    pub fn stats(&self) -> AppStats {
        let acts: Vec<&Activity> = self
            .filtered
            .iter()
            .filter_map(|&i| self.activities.get(i))
            .collect();

        let total_km: f64 = acts.iter().map(|a| a.distance_km()).sum();
        let total_secs: u64 = acts.iter().map(|a| a.moving_time).sum();
        let total_elev: f64 = acts.iter().map(|a| a.total_elevation_gain).sum();
        let total_kudos: u32 = acts.iter().map(|a| a.kudos_count).sum();

        let avg_pace = if total_km > 0.0 {
            let secs_per_km = total_secs as f64 / total_km;
            let m = secs_per_km as u64 / 60;
            let s = secs_per_km as u64 % 60;
            format!("{}'{:02}\"", m, s)
        } else {
            "-".to_string()
        };

        let longest = acts
            .iter()
            .max_by(|a, b| {
                a.distance
                    .partial_cmp(&b.distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|a| a.distance_km());

        AppStats {
            count: acts.len(),
            total_km,
            total_duration: format_duration(total_secs),
            total_elevation: total_elev,
            avg_pace,
            longest_km: longest.unwrap_or(0.0),
            total_kudos,
        }
    }
}

pub struct AppStats {
    pub count: usize,
    pub total_km: f64,
    pub total_duration: String,
    pub total_elevation: f64,
    pub avg_pace: String,
    pub longest_km: f64,
    pub total_kudos: u32,
}

fn format_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    format!("{}h{:02}m", h, m)
}
