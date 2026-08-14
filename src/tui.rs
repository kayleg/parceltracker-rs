use anyhow::Result;
use chrono::Utc;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, TableState, Wrap,
    },
    Frame, Terminal,
};
use std::io;
use std::sync::mpsc;

use crate::api::Client;
use crate::models::{Carrier, Parcel};
use crate::storage::{load_config, save_config, save_parcels};

fn format_eta_smart(eta_str: &str) -> String {
    use chrono::{DateTime, Datelike, Local, Weekday};

    let datetime = match DateTime::parse_from_rfc3339(eta_str) {
        Ok(dt) => dt.with_timezone(&Local),
        Err(_) => return eta_str.to_string(),
    };

    let now = Local::now();
    let today = now.date_naive();
    let delivery_date = datetime.date_naive();
    let days_diff = (delivery_date - today).num_days();
    let time_str = datetime.format("%-I:%M%p").to_string().to_lowercase();

    match days_diff {
        0 => format!("Today @ {}", time_str),
        1 => format!("Tomorrow @ {}", time_str),
        2..=6 => {
            let day_name = match datetime.weekday() {
                Weekday::Mon => "Mon",
                Weekday::Tue => "Tue",
                Weekday::Wed => "Wed",
                Weekday::Thu => "Thu",
                Weekday::Fri => "Fri",
                Weekday::Sat => "Sat",
                Weekday::Sun => "Sun",
            };
            format!("{} @ {}", day_name, time_str)
        }
        7..=365 => datetime
            .format("%b %d @ %-I:%M%p")
            .to_string()
            .to_lowercase(),
        _ => datetime
            .format("%b %d, %Y @ %-I:%M%p")
            .to_string()
            .to_lowercase(),
    }
}

#[derive(Debug, Clone)]
enum Command {
    Start(usize),
    ItemStart(usize),
    ItemDone(usize, Box<Parcel>, Option<String>),
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Add,
    Rename,
}

pub struct App {
    pub parcels: Vec<Parcel>,
    pub table_state: TableState,
    pub selected_id: Option<String>,
    pub show_details: bool,
    pub details_scroll: usize,
    pub details_scroll_state: ScrollbarState,
    pub message: Option<String>,
    pub quit: bool,
    pub launch_setup: bool,
    // Update tracking fields
    pub is_updating: bool,
    pub parcels_to_update: usize,
    pub parcels_updated: usize,
    pub updating_index: Option<usize>,
    pub spinner_tick: usize,
    pub row_errors: Vec<Option<String>>,
    input_mode: Option<InputMode>,
    input_buffer: String,
}

impl App {
    pub fn new(parcels: Vec<Parcel>) -> Result<Self> {
        let mut table_state = TableState::default();
        if !parcels.is_empty() {
            table_state.select(Some(0));
        }

        let config = load_config()?;

        let selected_id = if let Some(selection) = &config.waybar_selected {
            // Find parcel by tracking number and get its id
            parcels
                .iter()
                .find(|p| p.tracking_number == selection.tracking)
                .map(|p| p.id.clone())
        } else {
            // Find first non-delivered parcel
            parcels
                .iter()
                .find(|p| !p.is_delivered())
                .map(|p| p.id.clone())
        };

        Ok(Self {
            parcels,
            table_state,
            selected_id,
            show_details: false,
            details_scroll: 0,
            details_scroll_state: ScrollbarState::default(),
            message: None,
            quit: false,
            launch_setup: false,
            is_updating: false,
            parcels_to_update: 0,
            parcels_updated: 0,
            updating_index: None,
            spinner_tick: 0,
            row_errors: Vec::new(),
            input_mode: None,
            input_buffer: String::new(),
        })
    }

    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let res = self.run_app(&mut terminal);

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        res
    }

    fn run_app(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        let mut last_tick = std::time::Instant::now();
        let tick_rate = std::time::Duration::from_millis(250);

        let (command_tx, command_rx) = mpsc::channel::<Command>();
        self.start_update(command_tx.clone());

        while !self.quit {
            terminal.draw(|f| self.ui(f))?;

            while let Ok(cmd) = command_rx.try_recv() {
                self.handle_command(cmd)?;
            }

            let timeout = tick_rate
                .checked_sub(last_tick.elapsed())
                .unwrap_or_else(|| std::time::Duration::from_secs(0));

            if crossterm::event::poll(timeout)? {
                self.handle_event(&command_tx)?;
            }

            if self.launch_setup {
                self.launch_setup = false;
                self.run_setup(terminal)?;
                self.start_update(command_tx.clone());
            }

            if last_tick.elapsed() >= tick_rate {
                if self.is_updating {
                    self.spinner_tick = (self.spinner_tick + 1) % 10;
                }
                last_tick = std::time::Instant::now();
            }
        }

        Ok(())
    }

    /// Suspend the TUI, run the interactive credential wizard inline in
    /// the normal terminal screen, then restore the TUI. The caller
    /// triggers a refresh so new credentials take effect immediately.
    fn run_setup(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> Result<()> {
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(crate::setup::run())
        });

        {
            use std::io::Write;
            print!("\nPress Enter to return to the parcel list… ");
            io::stdout().flush()?;
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
        }

        enable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            EnterAlternateScreen,
            EnableMouseCapture
        )?;
        terminal.clear()?;

        self.message = Some(match result {
            Ok(()) => "Credentials saved — refreshing".to_string(),
            Err(e) => format!("Setup failed: {}", e),
        });
        Ok(())
    }

    fn start_update(&mut self, command_tx: mpsc::Sender<Command>) {
        if self.parcels.is_empty() || self.is_updating {
            return;
        }
        let parcels_clone = self.parcels.clone();
        tokio::spawn(async move {
            if let Err(e) = Self::update_all_parcels(parcels_clone, command_tx.clone()).await {
                let _ = command_tx.send(Command::Complete);
                eprintln!("Failed to update parcels: {}", e);
            }
        });
    }

    async fn update_all_parcels(
        parcels: Vec<Parcel>,
        command_tx: mpsc::Sender<Command>,
    ) -> Result<()> {
        let total = parcels.len();
        let _ = command_tx.send(Command::Start(total));
        let config = crate::storage::load_config().unwrap_or_default();
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        let client = Client::new().await?;

        // Register with 17track before fetching: gettrackinfo only returns data
        // for numbers that have already been registered.
        if client.register(&parcels).await.is_ok() {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        for (idx, parcel) in parcels.iter().enumerate() {
            let _ = command_tx.send(Command::ItemStart(idx));

            let mut updated_parcel = parcel.clone();
            // Same tiering as `parceltracker update`: first-party carrier
            // API when configured, 17track otherwise or on failure.
            let result =
                match crate::carriers::for_carrier(&updated_parcel.resolved_carrier(), &config) {
                    Some(provider) => {
                        match provider.track(&http, &updated_parcel.tracking_number).await {
                            Ok(info) => Ok(info),
                            Err(_) => client.get_tracking_info(&updated_parcel).await,
                        }
                    }
                    None => client.get_tracking_info(&updated_parcel).await,
                };
            let err = match result {
                Ok(info) => {
                    updated_parcel.tracking_info = Some(info);
                    updated_parcel.last_updated = Some(chrono::Utc::now());
                    None
                }
                Err(e) => Some(e.to_string()),
            };

            let _ = command_tx.send(Command::ItemDone(idx, Box::new(updated_parcel), err));
        }

        let _ = command_tx.send(Command::Complete);

        Ok(())
    }

    fn handle_command(&mut self, cmd: Command) -> Result<()> {
        match cmd {
            Command::Start(total) => {
                self.is_updating = true;
                self.parcels_to_update = total;
                self.parcels_updated = 0;
                self.updating_index = None;
                self.row_errors = vec![None; self.parcels.len()];
                self.message = Some("Updating parcels...".to_string());
            }
            Command::ItemStart(index) => {
                self.updating_index = Some(index);
            }
            Command::ItemDone(index, updated_parcel, err) => {
                if index < self.parcels.len() {
                    self.parcels[index] = *updated_parcel;
                    if self.row_errors.len() == self.parcels.len() {
                        self.row_errors[index] = err;
                    }
                }
                self.parcels_updated += 1;
                save_parcels(&self.parcels)?;
            }
            Command::Complete => {
                self.is_updating = false;
                self.updating_index = None;
                let failed = self
                    .row_errors
                    .iter()
                    .filter(|e| e.as_ref().is_some())
                    .count();
                if failed == 0 {
                    self.message = Some(format!("Updated {} parcel(s)", self.parcels_updated));
                } else {
                    self.message = Some(format!(
                        "Updated {} parcel(s), {} failed",
                        self.parcels_updated.saturating_sub(failed),
                        failed
                    ));
                }
            }
        }
        Ok(())
    }

    fn handle_event(&mut self, command_tx: &mpsc::Sender<Command>) -> Result<()> {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if let Some(mode) = self.input_mode {
                    match key.code {
                        KeyCode::Esc => {
                            self.input_mode = None;
                            self.input_buffer.clear();
                        }
                        KeyCode::Backspace => {
                            self.input_buffer.pop();
                        }
                        KeyCode::Enter => {
                            match mode {
                                InputMode::Add => {
                                    let trimmed = self.input_buffer.trim();
                                    if trimmed.is_empty() {
                                        self.message = Some("Add cancelled".to_string());
                                    } else {
                                        let mut parts = trimmed.splitn(2, ' ');
                                        let tracking =
                                            parts.next().unwrap_or("").trim().to_string();
                                        let description =
                                            parts.next().unwrap_or("").trim().to_string();
                                        if tracking.is_empty() {
                                            self.message =
                                                Some("Usage: <tracking> [description]".to_string());
                                        } else if self
                                            .parcels
                                            .iter()
                                            .any(|p| p.tracking_number == tracking)
                                        {
                                            self.message =
                                                Some("Tracking number already exists".to_string());
                                        } else {
                                            let carrier =
                                                Carrier::detect(&tracking).name().to_lowercase();
                                            let desc = if description.is_empty() {
                                                tracking.clone()
                                            } else {
                                                description
                                            };
                                            self.parcels.push(Parcel::new(tracking, desc, carrier));
                                            save_parcels(&self.parcels)?;
                                            self.row_errors = vec![None; self.parcels.len()];
                                            if self.table_state.selected().is_none() {
                                                self.table_state.select(Some(0));
                                            }
                                            self.message = Some("Parcel added".to_string());
                                        }
                                    }
                                }
                                InputMode::Rename => {
                                    if let Some(idx) = self.table_state.selected() {
                                        if idx < self.parcels.len() {
                                            let desc = self.input_buffer.trim();
                                            if !desc.is_empty() {
                                                self.parcels[idx].description = desc.to_string();
                                                save_parcels(&self.parcels)?;
                                                self.message = Some("Parcel renamed".to_string());
                                            }
                                        }
                                    }
                                }
                            }
                            self.input_mode = None;
                            self.input_buffer.clear();
                        }
                        KeyCode::Char(c) => {
                            self.input_buffer.push(c);
                        }
                        _ => {}
                    }
                    return Ok(());
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        if self.show_details {
                            self.show_details = false;
                            self.details_scroll = 0;
                        } else {
                            self.quit = true;
                        }
                    }
                    KeyCode::Char('u') => {
                        self.unselect()?;
                    }
                    KeyCode::Char('U') => {
                        self.start_update(command_tx.clone());
                    }
                    KeyCode::Char('a') if !self.show_details => {
                        self.input_mode = Some(InputMode::Add);
                        self.input_buffer.clear();
                    }
                    KeyCode::Char('r') if !self.show_details => {
                        if self.table_state.selected().is_some() {
                            self.input_mode = Some(InputMode::Rename);
                            self.input_buffer.clear();
                        }
                    }
                    KeyCode::Char('s') if !self.show_details => {
                        self.launch_setup = true;
                    }
                    KeyCode::Char('d') if !self.show_details => {
                        if let Some(idx) = self.table_state.selected() {
                            if idx < self.parcels.len() {
                                let removed = self.parcels.remove(idx);
                                save_parcels(&self.parcels)?;
                                if self.parcels.is_empty() {
                                    self.table_state.select(None);
                                    self.selected_id = None;
                                } else if idx >= self.parcels.len() {
                                    self.table_state.select(Some(self.parcels.len() - 1));
                                }
                                if self.selected_id.as_ref() == Some(&removed.id) {
                                    self.selected_id = None;
                                    self.save_selection()?;
                                }
                                self.row_errors = vec![None; self.parcels.len()];
                                self.message = Some("Parcel removed".to_string());
                            }
                        }
                    }
                    KeyCode::Char(c) if c.is_ascii_digit() && !self.show_details => {
                        let pos = c.to_digit(10).unwrap() as usize;
                        if pos > 0 && pos <= self.parcels.len() {
                            self.select_by_position(pos)?;
                        }
                    }
                    KeyCode::Up => {
                        if !self.show_details {
                            self.previous();
                        }
                    }
                    KeyCode::Down => {
                        if !self.show_details {
                            self.next();
                        }
                    }
                    KeyCode::Enter => {
                        if !self.show_details {
                            if let Some(idx) = self.table_state.selected() {
                                if idx < self.parcels.len() {
                                    self.show_details = true;
                                    self.details_scroll = 0;
                                    self.update_details_scroll_state();
                                }
                            }
                        }
                    }
                    KeyCode::PageUp => {
                        if self.show_details {
                            self.scroll_up(5);
                        }
                    }
                    KeyCode::PageDown => {
                        if self.show_details {
                            self.scroll_down(5);
                        }
                    }
                    KeyCode::Home => {
                        if self.show_details {
                            self.details_scroll = 0;
                            self.update_details_scroll_state();
                        }
                    }
                    KeyCode::End => {
                        if self.show_details {
                            // Will be set by content length
                        }
                    }
                    _ => {}
                }
            }
            Event::Mouse(mouse) => {
                use crossterm::event::MouseEventKind;
                match mouse.kind {
                    MouseEventKind::ScrollUp => {
                        if self.show_details {
                            self.scroll_up(3);
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if self.show_details {
                            self.scroll_down(3);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn select_by_position(&mut self, position: usize) -> Result<()> {
        let index = position - 1;
        if index < self.parcels.len() {
            self.selected_id = Some(self.parcels[index].id.clone());
            self.save_selection()?;
            self.message = Some(format!("Selected: {}", self.parcels[index].display_name()));
        }
        Ok(())
    }

    fn unselect(&mut self) -> Result<()> {
        self.selected_id = None;
        self.save_selection()?;
        self.message = Some("Selection cleared".to_string());
        Ok(())
    }

    fn save_selection(&self) -> Result<()> {
        let mut config = load_config()?;

        if let Some(id) = &self.selected_id {
            // Find the parcel to get its tracking number
            if let Some(parcel) = self.parcels.iter().find(|p| p.id == *id) {
                config.waybar_selected = Some(crate::models::WaybarSelection {
                    tracking: parcel.tracking_number.clone(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                });
            }
        } else {
            config.waybar_selected = None;
        }

        save_config(&config)?;
        Ok(())
    }

    fn previous(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.parcels.len().saturating_sub(1)
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    fn next(&mut self) {
        let i = match self.table_state.selected() {
            Some(i) => {
                if i >= self.parcels.len().saturating_sub(1) {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.table_state.select(Some(i));
    }

    fn scroll_up(&mut self, amount: usize) {
        self.details_scroll = self.details_scroll.saturating_sub(amount);
        self.update_details_scroll_state();
    }

    fn scroll_down(&mut self, amount: usize) {
        self.details_scroll += amount;
        self.update_details_scroll_state();
    }

    fn update_details_scroll_state(&mut self) {
        self.details_scroll_state = self.details_scroll_state.position(self.details_scroll);
    }

    fn ui(&mut self, frame: &mut Frame) {
        let main_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(3)])
            .margin(1)
            .split(frame.size());

        if self.show_details {
            self.render_details(frame, main_layout[0]);
        } else {
            self.render_table(frame, main_layout[0]);
        }

        self.render_help(frame, main_layout[1]);
    }

    fn render_table(&mut self, frame: &mut Frame, area: Rect) {
        let header_cells = ["#", "Status", "Description", "Tracking", "ETA"]
            .iter()
            .map(|h| {
                Cell::from(*h).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            });
        let header = Row::new(header_cells)
            .style(Style::default().add_modifier(Modifier::BOLD))
            .height(1);

        let rows: Vec<Row> = self
            .parcels
            .iter()
            .enumerate()
            .map(|(i, parcel)| {
                let is_selected_for_waybar = self.selected_id.as_ref() == Some(&parcel.id);
                let is_delivered = parcel.is_delivered();

                let num = format!("{}.", i + 1);
                let status_text = parcel
                    .tracking_info
                    .as_ref()
                    .map(|t| t.status_text())
                    .unwrap_or_else(|| "Not updated".to_string());
                let status = format!("{} {}", parcel.status_emoji(), status_text);

                let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                let status = if self.updating_index == Some(i) {
                    format!("{} {}", spinner[self.spinner_tick], status)
                } else if self.row_errors.get(i).and_then(|e| e.as_ref()).is_some() {
                    format!("✗ {}", status)
                } else {
                    status
                };

                let status_color = if is_delivered {
                    Color::Green
                } else {
                    Color::Cyan
                };

                let eta = parcel
                    .tracking_info
                    .as_ref()
                    .and_then(|info| {
                        info.estimated_delivery_date
                            .as_ref()
                            .map(|eta| format_eta_smart(&eta.to_rfc3339()))
                    })
                    .unwrap_or_else(|| "-".to_string());

                let desc = if is_selected_for_waybar {
                    format!("★ {}", parcel.description)
                } else {
                    parcel.description.clone()
                };

                let cells = vec![
                    Cell::from(num),
                    Cell::from(status).style(Style::default().fg(status_color)),
                    Cell::from(desc),
                    Cell::from(parcel.tracking_number.clone())
                        .style(Style::default().fg(Color::DarkGray)),
                    Cell::from(eta),
                ];

                Row::new(cells).height(1)
            })
            .collect();

        let title = if self.is_updating {
            format!(
                "Parcels · updating {}/{}",
                self.parcels_updated, self.parcels_to_update
            )
        } else {
            "Parcels · a:add r:ren d:del U:upd 1-9:sel u:clr Enter:details q:quit".to_string()
        };

        let table = Table::new(rows)
            .header(header)
            .widths(&[
                Constraint::Length(3),
                Constraint::Length(12),
                Constraint::Min(20),
                Constraint::Length(22),
                Constraint::Length(20),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_type(BorderType::Rounded),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");

        frame.render_stateful_widget(table, area, &mut self.table_state);
    }

    fn render_details(&mut self, frame: &mut Frame, area: Rect) {
        let idx = self.table_state.selected().unwrap_or(0);
        if idx >= self.parcels.len() {
            return;
        }

        let parcel = &self.parcels[idx];
        let is_waybar_selected = self.selected_id.as_ref() == Some(&parcel.id);

        let mut lines: Vec<Line> = vec![
            Line::from(vec![
                Span::styled(
                    "Tracking: ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(&parcel.tracking_number),
            ]),
            Line::from(vec![
                Span::styled(
                    "Description: ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(&parcel.description),
            ]),
            Line::from(vec![
                Span::styled(
                    "Carrier: ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(if parcel.carrier == "auto" {
                    format!(
                        "Auto-detected ({})",
                        Carrier::detect(&parcel.tracking_number).name()
                    )
                } else {
                    parcel.carrier.clone()
                }),
            ]),
            Line::from(vec![
                Span::styled(
                    "Waybar Selected: ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(if is_waybar_selected { "Yes ★" } else { "No" }),
            ]),
            Line::from(""),
        ];

        if let Some(info) = &parcel.tracking_info {
            lines.push(Line::from(vec![
                Span::styled(
                    "Status: ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} {}", parcel.status_emoji(), info.status_text()),
                    if info.is_delivered() {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::Cyan)
                    },
                ),
            ]));

            if let Some(location) = &info.current_location {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Current Location: ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(location),
                ]));
            }

            if let Some(eta) = info.estimated_delivery_date {
                let now = Utc::now().date_naive();
                let est = eta.date_naive();
                let days = (est - now).num_days();

                lines.push(Line::from(vec![
                    Span::styled(
                        "Estimated Delivery: ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("{} (", eta.format("%Y-%m-%d"))),
                    Span::styled(
                        if days < 0 {
                            format!("{} days overdue)", days.abs())
                        } else if days == 0 {
                            "today)".to_string()
                        } else {
                            format!("in {} days)", days)
                        },
                        if days < 0 {
                            Style::default().fg(Color::Red)
                        } else if days <= 2 {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default().fg(Color::Green)
                        },
                    ),
                ]));
            }

            if let Some(last_updated) = parcel.last_updated {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Last Updated: ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(last_updated.format("%Y-%m-%d %H:%M").to_string()),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                "Tracking History:",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(""));

            for event in &info.events {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("[{}] ", event.date.format("%Y-%m-%d %H:%M")),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(&event.description),
                ]));
            }
        } else {
            lines.push(Line::from(
                "No tracking information available. Run 'parceltracker update' to fetch.",
            ));
        }

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Details: {}", parcel.display_name()))
                    .border_type(BorderType::Rounded),
            )
            .wrap(Wrap { trim: true })
            .scroll((self.details_scroll as u16, 0));

        frame.render_widget(paragraph, area);

        // Render scrollbar
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        frame.render_stateful_widget(
            scrollbar,
            area.inner(&Margin {
                horizontal: 0,
                vertical: 1,
            }),
            &mut self.details_scroll_state,
        );
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let status = if let Some(mode) = self.input_mode {
            match mode {
                InputMode::Add => format!("Add parcel: {}", self.input_buffer),
                InputMode::Rename => format!("Rename parcel: {}", self.input_buffer),
            }
        } else if self.is_updating {
            let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            format!(
                "{} Updating {}/{}",
                spinner[self.spinner_tick], self.parcels_updated, self.parcels_to_update
            )
        } else {
            self.message.clone().unwrap_or_default()
        };

        let help_text: String = if self.show_details {
            format!(
                "{}  q/Esc: Close  ↑/↓: Scroll  PgUp/PgDn: Page  Home: Top  End: Bottom",
                status
            )
        } else {
            format!(
                "{}  ↑/↓: Navigate  Enter: Details  a:Add  r:Rename  d:Delete  U:Update  s:Setup keys  1-9:Select  u:Unselect  q:Quit",
                status
            )
        };

        let help = Paragraph::new(help_text)
            .block(Block::default().borders(Borders::TOP))
            .alignment(Alignment::Center);

        frame.render_widget(help, area);
    }
}
