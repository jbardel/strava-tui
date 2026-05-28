use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Bar, BarChart, BarGroup, Block, BorderType, Borders, Cell, Clear, Padding, Paragraph, Row,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Table, Tabs, Wrap,
    },
    Frame,
};

use crate::app::{App, Screen};

// Palette de couleurs Strava-inspired
const ORANGE: Color = Color::Rgb(252, 76, 2);
const ORANGE_DARK: Color = Color::Rgb(180, 50, 0);
const GRAY: Color = Color::Rgb(80, 80, 90);
const LIGHT_GRAY: Color = Color::Rgb(180, 180, 190);
const WHITE: Color = Color::White;
const GREEN: Color = Color::Rgb(39, 174, 96);
const BLUE: Color = Color::Rgb(52, 152, 219);
const YELLOW: Color = Color::Rgb(241, 196, 15);
const BG: Color = Color::Rgb(15, 15, 20);
const BG2: Color = Color::Rgb(25, 25, 35);
const BG3: Color = Color::Rgb(35, 35, 50);

pub fn render(f: &mut Frame, app: &App) {
    let size = f.size();

    // Background global
    f.render_widget(Block::default().style(Style::default().bg(BG)), size);

    // Layout principal
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Corps
            Constraint::Length(1), // Status bar
        ])
        .split(size);

    render_header(f, app, chunks[0]);

    match app.screen {
        Screen::List => render_list(f, app, chunks[1]),
        Screen::Detail => render_detail(f, app, chunks[1]),
        Screen::Stats => render_stats(f, app, chunks[1]),
        Screen::Help => render_help(f, app, chunks[1]),
    }

    render_statusbar(f, app, chunks[2]);

    // Overlay de chargement
    if app.loading {
        render_loading(f, size);
    }

    // Overlay d'erreur
    if let Some(err) = &app.error {
        render_error(f, err, size);
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let tabs_titles: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("📋 Liste", Style::default().fg(WHITE)),
            Span::styled("  ", Style::default()),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("📊 Stats", Style::default().fg(WHITE)),
            Span::styled("  ", Style::default()),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("❓ Aide", Style::default().fg(WHITE)),
            Span::styled("  ", Style::default()),
        ]),
    ];

    let selected_tab = match app.screen {
        Screen::List | Screen::Detail => 0,
        Screen::Stats => 1,
        Screen::Help => 2,
    };

    let tabs = Tabs::new(tabs_titles)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(ORANGE))
                .border_type(BorderType::Plain)
                .title(Span::styled(
                    " 🏃 STRAVA TUI ",
                    Style::default().fg(ORANGE).add_modifier(Modifier::BOLD),
                ))
                .title_alignment(Alignment::Left)
                .style(Style::default().bg(BG2)),
        )
        .select(selected_tab)
        .style(Style::default().fg(LIGHT_GRAY).bg(BG2))
        .highlight_style(
            Style::default()
                .fg(ORANGE)
                .bg(BG3)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled("|", Style::default().fg(GRAY)));

    f.render_widget(tabs, area);
}

fn render_list(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Barre de filtre/tri
            Constraint::Min(0),    // Liste
        ])
        .split(area);

    render_filter_bar(f, app, chunks[0]);

    if app.filtered.is_empty() && !app.loading {
        let msg = if app.search_query.is_empty() {
            "Aucune activité trouvée"
        } else {
            "Aucun résultat pour cette recherche"
        };
        let p = Paragraph::new(msg)
            .alignment(Alignment::Center)
            .style(Style::default().fg(GRAY))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(GRAY)),
            );
        f.render_widget(p, chunks[1]);
        return;
    }

    // Divise en liste + détail rapide
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[1]);

    render_activity_table(f, app, body_chunks[0]);
    render_quick_detail(f, app, body_chunks[1]);
}

fn render_filter_bar(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    // Barre de recherche
    let search_style = if app.search_mode {
        Style::default().fg(ORANGE)
    } else {
        Style::default().fg(GRAY)
    };
    let search_text = if app.search_mode {
        format!(" 🔍 {} █", app.search_query)
    } else if app.search_query.is_empty() {
        " 🔍 Rechercher (/)".to_string()
    } else {
        format!(" 🔍 {}", app.search_query)
    };
    let search = Paragraph::new(search_text).style(search_style).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(search_style)
            .style(Style::default().bg(BG2)),
    );
    f.render_widget(search, chunks[0]);

    // Filtre
    let filter = Paragraph::new(format!(" 🏷️  {} (f)", app.filter_label()))
        .style(Style::default().fg(LIGHT_GRAY))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(GRAY))
                .style(Style::default().bg(BG2)),
        );
    f.render_widget(filter, chunks[1]);

    // Tri
    let order_arrow = if app.sort_desc { "↓" } else { "↑" };
    let sort = Paragraph::new(format!(" 🔃 {} {} (s/r)", app.sort_label(), order_arrow))
        .style(Style::default().fg(LIGHT_GRAY))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(GRAY))
                .style(Style::default().bg(BG2)),
        );
    f.render_widget(sort, chunks[2]);
}

fn render_activity_table(f: &mut Frame, app: &App, area: Rect) {
    let visible = area.height.saturating_sub(3) as usize;

    let header_cells = ["Date", "Nom", "Dist.", "Durée", "Allure"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells)
        .style(Style::default().bg(BG3))
        .height(1);

    let rows: Vec<Row> = app
        .filtered
        .iter()
        .skip(app.scroll_offset)
        .take(visible)
        .enumerate()
        .filter_map(|(i, &idx)| {
            let a = app.activities.get(idx)?;
            let is_selected = (i + app.scroll_offset) == app.selected;

            let style = if is_selected {
                Style::default()
                    .fg(WHITE)
                    .bg(ORANGE_DARK)
                    .add_modifier(Modifier::BOLD)
            } else if i % 2 == 0 {
                Style::default().fg(LIGHT_GRAY).bg(BG2)
            } else {
                Style::default().fg(LIGHT_GRAY).bg(BG)
            };

            let icon = a.type_icon();
            Some(
                Row::new(vec![
                    Cell::from(a.date_short()),
                    Cell::from(format!("{} {}", icon, truncate(&a.name, 22))),
                    Cell::from(format!("{:.2} km", a.distance_km())),
                    Cell::from(a.duration_formatted()),
                    Cell::from(a.pace_per_km()),
                ])
                .style(style)
                .height(1),
            )
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Min(20),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Length(8),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!(
                " 🏃 Activités ({}/{}) ",
                app.filtered.len(),
                app.activities.len()
            ))
            .title_style(Style::default().fg(ORANGE))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(GRAY))
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(BG2)),
    );

    f.render_widget(table, area);

    // Scrollbar
    if app.filtered.len() > visible {
        let mut scroll_state = ScrollbarState::new(app.filtered.len()).position(app.scroll_offset);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area,
            &mut scroll_state,
        );
    }
}

fn render_quick_detail(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" 📍 Détail ")
        .title_style(Style::default().fg(ORANGE))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(GRAY))
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(BG2));

    if let Some(a) = app.selected_activity() {
        let hr_str = a
            .average_heartrate
            .map(|hr| format!("{:.0} bpm", hr))
            .unwrap_or_else(|| "-".to_string());
        let max_hr_str = a
            .max_heartrate
            .map(|hr| format!("{:.0} bpm", hr))
            .unwrap_or_else(|| "-".to_string());
        let suffer = a
            .suffer_score
            .map(|s| format!("{:.0}", s))
            .unwrap_or_else(|| "-".to_string());

        let lines = vec![
            Line::from(vec![
                Span::styled(format!("{} ", a.type_icon()), Style::default()),
                Span::styled(
                    a.name.clone(),
                    Style::default().fg(ORANGE).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            stat_line(
                "📅 Date",
                &a.start_date.format("%d/%m/%Y %H:%M").to_string(),
            ),
            Line::from(""),
            stat_line("📏 Distance", format!("{:.2} km", a.distance_km())),
            stat_line("⏱️  Durée", a.duration_formatted()),
            stat_line("⚡ Allure moy.", a.pace_per_km()),
            stat_line("🚀 Vitesse max", format!("{:.1} km/h", a.max_speed * 3.6)),
            Line::from(""),
            stat_line("⛰️  Dénivelé", format!("{:.0} m", a.total_elevation_gain)),
            Line::from(""),
            stat_line("❤️  FC moy.", hr_str),
            stat_line("💥 FC max", max_hr_str),
            stat_line("😓 Suffer score", suffer),
            Line::from(""),
            stat_line("👍 Kudos", format!("{}", a.kudos_count)),
            stat_line("🏅 Achievements", format!("{}", a.achievement_count)),
            Line::from(""),
            Line::from(vec![Span::styled(
                " → Entrée pour détail complet",
                Style::default().fg(GRAY).add_modifier(Modifier::ITALIC),
            )]),
        ];

        let para = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
        f.render_widget(para, area);
    } else {
        let para = Paragraph::new("Sélectionnez une activité")
            .alignment(Alignment::Center)
            .style(Style::default().fg(GRAY))
            .block(block);
        f.render_widget(para, area);
    }
}

fn render_detail(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" 📋 Détail complet (Échap pour revenir) ")
        .title_style(Style::default().fg(ORANGE))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ORANGE))
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(BG2))
        .padding(Padding::uniform(1));

    if let Some(a) = app.selected_activity() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(0)])
            .margin(1)
            .split(area);

        // Titre + type
        let title = Paragraph::new(vec![
            Line::from(vec![
                Span::styled(format!("{} ", a.type_icon()), Style::default().fg(WHITE)),
                Span::styled(
                    a.name.clone(),
                    Style::default()
                        .fg(ORANGE)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                ),
            ]),
            Line::from(vec![Span::styled(
                format!(
                    "  {} • {}",
                    a.activity_type,
                    a.start_date.format("%A %d %B %Y à %H:%M")
                ),
                Style::default().fg(LIGHT_GRAY),
            )]),
        ])
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(GRAY)),
        );

        f.render_widget(block, area);
        f.render_widget(title, chunks[0]);

        // Grille de stats en 2 colonnes
        let col_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        let left = vec![
            Line::from(""),
            section_title("🏃 Performance"),
            stat_line("Distance", format!("{:.3} km", a.distance_km())),
            stat_line("Durée", a.duration_formatted()),
            stat_line("Allure moy.", a.pace_per_km()),
            stat_line("Vitesse moy.", format!("{:.2} km/h", a.average_speed * 3.6)),
            stat_line("Vitesse max", format!("{:.2} km/h", a.max_speed * 3.6)),
            Line::from(""),
            section_title("⛰️  Terrain"),
            stat_line(
                "Dénivelé positif",
                format!("{:.0} m", a.total_elevation_gain),
            ),
        ];

        let hr_str = a
            .average_heartrate
            .map(|hr| format!("{:.0} bpm", hr))
            .unwrap_or("-".into());
        let max_hr_str = a
            .max_heartrate
            .map(|hr| format!("{:.0} bpm", hr))
            .unwrap_or("-".into());
        let suffer = a
            .suffer_score
            .map(|s| format!("{:.0}", s))
            .unwrap_or("-".into());

        let right = vec![
            Line::from(""),
            section_title("❤️  Cardio"),
            stat_line("FC moyenne", hr_str),
            stat_line("FC max", max_hr_str),
            stat_line("Suffer score", suffer),
            Line::from(""),
            section_title("🏅 Social"),
            stat_line("Kudos", format!("{}", a.kudos_count)),
            stat_line("Achievements", format!("{}", a.achievement_count)),
        ];

        let left_para = Paragraph::new(left).wrap(Wrap { trim: false });
        let right_para = Paragraph::new(right).wrap(Wrap { trim: false });

        f.render_widget(left_para, col_chunks[0]);
        f.render_widget(right_para, col_chunks[1]);
    }
}

fn render_stats(f: &mut Frame, app: &App, area: Rect) {
    let s = app.stats();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(0)])
        .split(area);

    // Ligne de KPIs
    let kpi_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(chunks[0]);

    render_kpi(
        f,
        "🏃 Activités",
        &format!("{}", s.count),
        ORANGE,
        kpi_chunks[0],
    );
    render_kpi(
        f,
        "📏 Distance totale",
        &format!("{:.1} km", s.total_km),
        BLUE,
        kpi_chunks[1],
    );
    render_kpi(
        f,
        "⏱️  Temps total",
        &s.total_duration,
        GREEN,
        kpi_chunks[2],
    );
    render_kpi(
        f,
        "⛰️  Dénivelé total",
        &format!("{:.0} m", s.total_elevation),
        YELLOW,
        kpi_chunks[3],
    );

    // Deuxième ligne : allure moy / record / kudos / longest
    let kpi2_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(chunks[1]);

    // Mini histogramme des distances par activité
    if !app.filtered.is_empty() {
        let hist_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(chunks[1]);

        let kpi2 = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
            ])
            .split(hist_chunks[0]);

        render_kpi(
            f,
            "⚡ Allure moy.",
            &s.avg_pace,
            Color::Rgb(155, 89, 182),
            kpi2[0],
        );
        render_kpi(
            f,
            "🏆 Plus longue",
            &format!("{:.2} km", s.longest_km),
            ORANGE,
            kpi2[1],
        );
        render_kpi(
            f,
            "👍 Kudos total",
            &format!("{}", s.total_kudos),
            Color::Rgb(231, 76, 60),
            kpi2[2],
        );

        // Histogramme des 20 dernières activités
        let bars: Vec<Bar> = app
            .filtered
            .iter()
            .take(20)
            .rev()
            .filter_map(|&i| app.activities.get(i))
            .map(|a| {
                Bar::default()
                    .value((a.distance_km() * 10.0) as u64)
                    .label(Line::from(truncate_short(&a.date_short()[..5], 5)))
                    .style(Style::default().fg(ORANGE))
                    .value_style(Style::default().fg(WHITE).bg(ORANGE))
            })
            .collect();

        let bar_group = BarGroup::default().bars(&bars);
        let barchart = BarChart::default()
            .block(
                Block::default()
                    .title(" 📊 Distances (20 dernières activités, km×10) ")
                    .title_style(Style::default().fg(ORANGE))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(GRAY))
                    .border_type(BorderType::Rounded)
                    .style(Style::default().bg(BG2)),
            )
            .data(bar_group)
            .bar_width(5)
            .bar_gap(1)
            .direction(Direction::Vertical);

        f.render_widget(barchart, hist_chunks[1]);
    }
}

fn render_kpi(f: &mut Frame, title: &str, value: &str, color: Color, area: Rect) {
    let content = vec![
        Line::from(""),
        Line::from(Span::styled(
            value,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(title, Style::default().fg(LIGHT_GRAY))),
    ];
    let para = Paragraph::new(content).alignment(Alignment::Center).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color))
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(BG2)),
    );
    f.render_widget(para, area);
}

fn render_help(f: &mut Frame, _app: &App, area: Rect) {
    let shortcuts = vec![
        (
            "Navigation",
            vec![
                ("↑/k", "Remonter"),
                ("↓/j", "Descendre"),
                ("Entrée", "Voir détail"),
                ("Échap", "Retour à la liste"),
            ],
        ),
        (
            "Filtres & Tri",
            vec![
                ("f", "Changer le filtre (Tout/Courses)"),
                ("s", "Changer le critère de tri"),
                ("r", "Inverser l'ordre de tri"),
                ("/", "Mode recherche"),
            ],
        ),
        (
            "Vues",
            vec![
                ("1", "Vue Liste"),
                ("2", "Vue Statistiques"),
                ("?/h", "Aide"),
            ],
        ),
        (
            "Données",
            vec![("Tab", "Charger plus d'activités"), ("q/Ctrl+C", "Quitter")],
        ),
    ];

    let mut lines: Vec<Line> = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  ⌨️  Raccourcis clavier",
            Style::default()
                .fg(ORANGE)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )]),
        Line::from(""),
    ];

    for (section, keys) in shortcuts {
        lines.push(Line::from(vec![Span::styled(
            format!("  {}", section),
            Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
        )]));
        for (key, desc) in keys {
            lines.push(Line::from(vec![
                Span::styled(format!("    {:15}", key), Style::default().fg(ORANGE)),
                Span::styled(desc, Style::default().fg(LIGHT_GRAY)),
            ]));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(vec![Span::styled(
        "  💡 Configuration : créez un fichier .env avec STRAVA_CLIENT_ID et STRAVA_CLIENT_SECRET",
        Style::default().fg(GRAY).add_modifier(Modifier::ITALIC),
    )]));

    let para = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" ❓ Aide ")
                .title_style(Style::default().fg(ORANGE))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(GRAY))
                .border_type(BorderType::Rounded)
                .style(Style::default().bg(BG2)),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(para, area);
}

fn render_statusbar(f: &mut Frame, app: &App, area: Rect) {
    let msg = if let Some(err) = &app.error {
        Span::styled(format!(" ❌ {}", err), Style::default().fg(Color::Red))
    } else if let Some(status) = &app.status_msg {
        Span::styled(format!(" ℹ️  {}", status), Style::default().fg(GREEN))
    } else if app.loading {
        Span::styled(" ⟳ Chargement...", Style::default().fg(YELLOW))
    } else {
        Span::styled(
            format!(
                " {} activités • {} filtrées • Tab: charger plus • q: quitter",
                app.activities.len(),
                app.filtered.len()
            ),
            Style::default().fg(GRAY),
        )
    };

    let bar = Paragraph::new(Line::from(msg)).style(Style::default().bg(BG3));
    f.render_widget(bar, area);
}

fn render_loading(f: &mut Frame, area: Rect) {
    let popup = centered_rect(30, 10, area);
    f.render_widget(Clear, popup);
    let block = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  ⟳ Chargement des activités...",
            Style::default().fg(ORANGE).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Connexion à l'API Strava",
            Style::default().fg(LIGHT_GRAY),
        )),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ORANGE))
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(BG2)),
    );
    f.render_widget(block, popup);
}

fn render_error(f: &mut Frame, err: &str, area: Rect) {
    let popup = centered_rect(50, 20, area);
    f.render_widget(Clear, popup);
    let block = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "  ❌ Erreur",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", err),
            Style::default().fg(WHITE),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Appuyez sur une touche pour continuer",
            Style::default().fg(GRAY),
        )),
    ])
    .wrap(Wrap { trim: true })
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .border_type(BorderType::Rounded)
            .style(Style::default().bg(BG2)),
    );
    f.render_widget(block, popup);
}

// Helpers
fn stat_line(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:20}", label), Style::default().fg(GRAY)),
        Span::styled(
            value.into(),
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn section_title(title: &str) -> Line {
    Line::from(Span::styled(
        title,
        Style::default()
            .fg(ORANGE)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    ))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", &s.chars().take(max - 1).collect::<String>())
    }
}

fn truncate_short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
