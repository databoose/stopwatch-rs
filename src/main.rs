use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval_at, Duration, Instant};
use tokio::time;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::widgets::Padding;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Alignment,
    style::{Color, Style, Stylize},
    text::Span,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

static UI_UPDATE_RATE_MS: u64 = 80;

#[derive(Clone)]
struct Time {
    second: u16,
    minute: u16,
    hour: u16,
    days: u16,
}

impl Time {
    fn new() -> Self {
        Self {
            second: 0,
            minute: 0,
            hour: 0,
            days: 0,
        }
    }
}

#[derive(Clone)]
struct Timer {
    time: Arc<Mutex<Time>>,
    label: Option<String>,
}

impl Timer {
    fn new(label: Option<String>) -> Self {
        Self {
            time: Arc::new(Mutex::new(Time::new())),
            label,
        }
    }
}

struct State {
    timers: Vec<Timer>,
    selected_timer: usize,
    input_mode: bool,
    input_buffer: String,
    show_help: bool,
}

impl State {
    fn new() -> Self {
        let args: Vec<String> = env::args().collect();
        let initial_label = if args.len() == 2 {
            Some(args[1].clone())
        } else {
            None
        };

        Self {
            timers: vec![Timer::new(initial_label)],
            selected_timer: 0,
            input_mode: false,
            input_buffer: String::new(),
            show_help: true,
        }
    }

    fn add_timer(&mut self) {
        if self.timers.len() < 6 {
            self.timers.push(Timer::new(None));
            self.selected_timer = self.timers.len() - 1;
        }
    }

    fn remove_timer(&mut self) {
        if self.timers.len() > 1 {
            self.timers.remove(self.selected_timer);
            if self.selected_timer >= self.timers.len() {
                self.selected_timer = self.timers.len() - 1;
            }
        }
    }

    fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    fn next_timer(&mut self) {
        if !self.timers.is_empty() {
            self.selected_timer = (self.selected_timer + 1) % self.timers.len(); // wraps around, moves from timer 0 → 1 → 2 → 3 → back to 0
        }
    }

    fn prev_timer(&mut self) {
        if !self.timers.is_empty() {
            if self.selected_timer == 0 {
                self.selected_timer = self.timers.len() - 1;
            } else {
                self.selected_timer -= 1;
            }
        }
    }

    fn set_label(&mut self) {
        if self.input_buffer.is_empty() {
            self.timers[self.selected_timer].label = None;
        } else {
            self.timers[self.selected_timer].label = Some(self.input_buffer.clone());
        }
        self.input_buffer.clear();
        self.input_mode = false;
    }
}

async fn counter(time: Arc<Mutex<Time>>) {
    let start = Instant::now() + Duration::from_secs(1);

    // automatically accounts for any computational time taken in the loop, mitigating drift
    // if computation takes 0.2s, the next wait is 0.8s to hit the 1s mark
    let mut interval = time::interval_at(start, Duration::from_secs(1));

    loop {
        interval.tick().await;

        let mut time_guard = time.lock().await;
        time_guard.second += 1;

        if time_guard.second > 59 {
            time_guard.second = 0;
            time_guard.minute += 1;

            if time_guard.minute > 59 {
                time_guard.minute = 0;
                time_guard.hour += 1;

                if time_guard.hour > 23 {
                    time_guard.hour = 0;
                    time_guard.days += 1;
                }
            }
        }
    }
}

fn get_layout_areas(frame: &Frame, timer_count: usize) -> Vec<Rect> {
    let area = frame.area();

    match timer_count {
        1 => vec![area],
        2 => {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            vec![chunks[0], chunks[1]]
        },
        3 => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);

            let top_halves = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[0]);

            let bottom_halves = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[1]);

            vec![top_halves[0], top_halves[1], bottom_halves[1]]
        },
        4 => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);

            let top_halves = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[0]);

            let bottom_halves = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[1]);

            vec![top_halves[0], top_halves[1], bottom_halves[1], bottom_halves[0]]
        },
        5 => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);

            let top_thirds = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(33), Constraint::Percentage(33), Constraint::Percentage(34)])
                .split(rows[0]);

            let bottom_halves = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[1]);

            vec![top_thirds[0], top_thirds[1], top_thirds[2], bottom_halves[0], bottom_halves[1]]
        },
        6 => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);

            let top_thirds = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(33), Constraint::Percentage(33), Constraint::Percentage(34)])
                .split(rows[0]);

            let bottom_thirds = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(33), Constraint::Percentage(33), Constraint::Percentage(34)])
                .split(rows[1]);

            vec![top_thirds[0], top_thirds[1], top_thirds[2], bottom_thirds[0], bottom_thirds[1], bottom_thirds[2]]
        },
        _ => vec![area],
    }
}

fn draw_timer(frame: &mut Frame, area: Rect, timer: &Timer, time_snapshot: &Time, index: usize, is_selected: bool, app: &State) {
    let border_color = if is_selected {
        Color::Green
    }
    else {
        Color::Gray
    };

    let mut title = format!(" Timer {} ", index + 1);
    let time_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title)
        .padding(Padding::uniform(1));

    let time_display = if app.input_mode && is_selected {
        format!("Label: {}_", app.input_buffer)
    } else {
        let time_str = format!(
            "{}d:{}h:{}m:{}s",
            time_snapshot.days,
            time_snapshot.hour,
            time_snapshot.minute,
            time_snapshot.second
        );

        match timer.label.as_ref() {
            None => time_str,
            Some(label) => format!("{}\n{}", time_str, label),
        }
    };

    let time_text = Paragraph::new(time_display)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray))
        .block(time_block);

    frame.render_widget(time_text, area);
}

fn draw_help(frame: &mut Frame) {
    let area = frame.area();
    let help_text = vec![
        "Shortcuts:",
        "  q     - Quit",
        "  a     - Add timer (max 6)",
        "  d     - Delete selected timer",
        "  tab   - Next timer",
        "  s-tab - Previous timer",
        "  l     - Set label for timer",
        "  h     - Toggle help",
        "  esc   - Cancel input",
    ];

    let help_area = Rect {
        x: area.width.saturating_sub(42), // position 42 chars from the right edge of screen
        y: area.height.saturating_sub(11), // position 11 lines from the bottom of screen

        width: (area.width / 4).max(38).min(area.width),
        height: (area.height / 3).max(10).min(area.height),
    };

    let help_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Help ");

    let help_paragraph = Paragraph::new(help_text.join("\n"))
        .block(help_block)
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(help_paragraph, help_area);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    let mut state = State::new();

    for timer in &state.timers {
        let counter_time = Arc::clone(&timer.time);

        tokio::spawn(async move {
            counter(counter_time).await;
        });
    }
    let mut interval = time::interval_at(Instant::now(), Duration::from_millis(UI_UPDATE_RATE_MS));

    loop {
        interval.tick().await;

        let mut time_snapshots = Vec::new();
        for timer in &state.timers {
            let time_guard = timer.time.lock().await;
            time_snapshots.push(time_guard.clone());
        }

        terminal.draw(|frame| {
            let areas = get_layout_areas(frame, state.timers.len());

            for i in 0..state.timers.len() {
                let is_selected = i == state.selected_timer;
                draw_timer(frame, areas[i], &state.timers[i], &time_snapshots[i], i, is_selected, &state);
            }

            if state.show_help {
                draw_help(frame);
            }
        })?;

        if crossterm::event::poll(Duration::from_millis(UI_UPDATE_RATE_MS))? {
            if let Event::Key(key) = event::read()? {
                if state.input_mode {
                    match key.code {
                        KeyCode::Enter => {
                            state.set_label();
                        },
                        KeyCode::Esc => {
                            state.input_mode = false;
                            state.input_buffer.clear();
                        },
                        KeyCode::Backspace => {
                            state.input_buffer.pop();
                        },
                        KeyCode::Char(c) => {
                            state.input_buffer.push(c);
                        },
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('a') => {
                            if state.timers.len() < 6 {
                                state.add_timer();

                                let timer = &state.timers[state.timers.len() - 1];
                                let counter_time = Arc::clone(&timer.time);

                                tokio::spawn(async move {
                                    counter(counter_time).await;
                                });
                            }
                        },
                        KeyCode::Char('d') => {
                            if state.timers.len() > 1 {
                                state.remove_timer();
                            }
                        },
                        KeyCode::Char('h') => {
                            state.toggle_help();
                        },
                        KeyCode::Tab => {
                            if key.modifiers.contains(KeyModifiers::SHIFT) {
                                state.prev_timer();
                            } else {
                                state.next_timer();
                            }
                        },
                        KeyCode::BackTab => {
                            state.prev_timer();
                        },
                        KeyCode::Char('l') => {
                            state.input_mode = true;
                            state.input_buffer.clear();
                        },
                        _ => {}
                    }
                }
            }
        }
    }

    ratatui::restore();
    Ok(())
}
