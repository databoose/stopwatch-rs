use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval_at, Duration, Instant, Interval};
use tokio::time;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::enable_raw_mode;
use ratatui::widgets::Padding;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Alignment,
    style::{Color, Style},
    text::{Span, Line},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

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

struct Timer {
    timer_state: Arc<Mutex<Time>>,
    label: Option<String>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
}

impl Timer {
    fn new(label: Option<String>) -> Self {
        Self {
            timer_state: Arc::new(Mutex::new(Time::new())),
            label,
            task_handle: None,
        }
    }
}

struct State {
    timers: Vec<Timer>,
    selected_timer: usize,
    ui_update_rate_ms: u64,
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
            ui_update_rate_ms: 50, // default update rate for ui thread is 50ms (20 FPS)
            input_mode: false,
            input_buffer: String::new(),

            show_help: true,
        }
    }

    fn add_timer(&mut self) {
        if self.timers.len() < 8 {
            self.timers.push(Timer::new(None));
            self.selected_timer = self.timers.len() - 1;
        }
    }

    fn remove_timer(&mut self) {
        if self.timers.len() > 1 {
            match &self.timers[self.selected_timer].task_handle {
                Some(handle) => handle.abort(),
                None => {}
            }

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

// bloated function, lot of redundant shit might find a way to clean up later idk, i think a max of 8 timers is solid
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
        7 => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);

            let top_fourths = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25)])
                .split(rows[0]);

            let bottom_thirds = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(33), Constraint::Percentage(33), Constraint::Percentage(34)])
                .split(rows[1]);

            vec![top_fourths[0], top_fourths[1], top_fourths[2], top_fourths[3], bottom_thirds[0], bottom_thirds[1], bottom_thirds[2]]
        },
        8 => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);

            let top_fourths = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25)])
                .split(rows[0]);

            let bottom_thirds = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25), Constraint::Percentage(25)])
                .split(rows[1]);

            vec![top_fourths[0], top_fourths[1], top_fourths[2], top_fourths[3], bottom_thirds[0], bottom_thirds[1], bottom_thirds[2], bottom_thirds[3]]
        }
        _ => vec![area],
    }
}

fn draw_timer_box(frame: &mut Frame, area: Rect, timer: &Timer, time_snapshot: &Time, index: usize, state: &State) {
    let is_selected = index == state.selected_timer;
    let border_color = if is_selected {
        Color::Green
    }
    else {
        Color::Gray
    };

    let title = format!(" Timer {} ", index + 1);
    let time_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title)
        .padding(Padding::uniform(1));

    let time_display = if state.input_mode && is_selected {
        format!("Label: {}_", state.input_buffer) // TODO : add input mode text wrapping for long labels
    } else {
        let time_str = format!(
            "{}d:{}h:{}m:{}s",
            time_snapshot.days,
            time_snapshot.hour,
            time_snapshot.minute,
            time_snapshot.second
        );

        match &timer.label {
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

fn draw_confirmation_prompt(frame: &mut Frame) {
    let area = frame.area();

    let line = Line::from(vec![
            Span::raw("Are you sure? "),
            Span::styled(
                "Y",
                Color::Green,
            ),
            Span::styled(
                "/",
                Color::Gray,
            ),
            Span::styled(
                "N",
                Color::Red,
            ),
    ]);

    let prompt_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Gray))
        .title(" Confirmation ");

    let prompt_paragraph = Paragraph::new(line)
        .block(prompt_block)
        .centered()
        .style(Style::default().fg(Color::Gray).bg(Color::Black));

    let rect_width = area.width / 2;
    let rect_height = area.height / 4;

    let x_pos = (area.width - rect_width) / 2;
    let y_pos = (area.height - rect_height) / 2;

    let prompt_area = Rect::new(
        x_pos,
        y_pos,
        rect_width,
        rect_height);

    frame.render_widget(prompt_paragraph, prompt_area);
}

fn draw_help(frame: &mut Frame, update_rate: u64) {
    let area = frame.area();
    let fps = 1000 / update_rate;
    let help_text = vec![
        "Shortcuts:",
        "  ctrl + q   - Quit",
        "  ctrl + a   - Add timer (max 8)",
        "  ctrl + d   - Delete selected timer",
        "  tab   - Next timer",
        "  l     - Set label for timer",
        "  h     - Toggle help",
        "  ↑/↓   - Increase/Decrease UI FPS",
        "  esc   - Cancel input",
    ];

    let help_area = Rect {
        x: area.width.saturating_sub(42), // position 42 chars from the right edge of screen
        y: area.height.saturating_sub(13), // position 13 chars from the bottom of screen

        width: (area.width / 4).max(38).min(area.width),
        height: (area.height / 3).max(10).min(area.height),
    };

    let help_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title_top(Line::from("Help").left_aligned())
        .title_top(Line::from(format!("FPS: {}", fps)).right_aligned());

    let help_paragraph = Paragraph::new(help_text.join("\n"))
        .block(help_block)
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(help_paragraph, help_area);
}

// since the only explicit tasks we spawn are simple counters, we can use lower threadcount than normal tbh
#[tokio::main(worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut terminal = ratatui::init();
    let mut state = State::new();

    // only iterates once because we only have one timer rn, might implement multiple args later
    for timer in &mut state.timers {
        let time_counter = Arc::clone(&timer.timer_state);

        let handle = tokio::spawn(async move {
            counter(time_counter).await;
        });

        timer.task_handle = Some(handle)
    }

    let mut interval = time::interval_at(Instant::now(), Duration::from_millis(state.ui_update_rate_ms));
    'main_loop: loop {
        interval.tick().await;

        // take snapshots of all timer states so we can draw them for next frame
        let mut time_snapshots = Vec::new();
        for timer in &state.timers {
            let time_guard = timer.timer_state.lock().await;
            time_snapshots.push(time_guard.clone());
        }

        terminal.draw(|frame| {
            let areas = get_layout_areas(frame, state.timers.len());

            for i in 0..state.timers.len() {
                draw_timer_box(frame, areas[i], &state.timers[i], &time_snapshots[i], i, &state);
            }

            if state.show_help {
                draw_help(frame, state.ui_update_rate_ms);
            }
        })?;

        if crossterm::event::poll(Duration::ZERO)? {
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
                        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            'confirm_loop: loop {
                                terminal.draw(|frame| { draw_confirmation_prompt(frame); })?;
                                if let Event::Key(key) = event::read()? {
                                        match key.code {
                                            KeyCode::Char('y') => break 'main_loop,
                                            KeyCode::Char('n') => break 'confirm_loop,
                                            _ => continue
                                        }
                                }
                            }
                        },
                        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if state.timers.len() < 8 {
                                state.add_timer();

                                let idx = state.timers.len() - 1;
                                let timer = &mut state.timers[idx];

                                let counter_time = Arc::clone(&timer.timer_state);
                                let handle = tokio::spawn(async move {
                                    counter(counter_time).await;
                                });

                                timer.task_handle = Some(handle);
                            }
                        },
                        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if state.timers.len() > 1 {
                                state.remove_timer();
                            }
                        },
                        KeyCode::Char('h') => {
                            state.toggle_help();
                        },
                        KeyCode::Char('l') => {
                            state.input_mode = true;
                            state.input_buffer.clear();
                        },
                        KeyCode::Up => {
                            if !(state.ui_update_rate_ms <= 10) { // cap at 100fps
                                state.ui_update_rate_ms = state.ui_update_rate_ms.saturating_sub(5);
                                interval = time::interval_at(Instant::now(), Duration::from_millis(state.ui_update_rate_ms));
                            }
                        },
                        KeyCode::Down => {
                            if !(state.ui_update_rate_ms >= 100) { // no lower than 10fps because it starts being unresponsive to key events
                                state.ui_update_rate_ms = state.ui_update_rate_ms.saturating_add(5);
                                interval = time::interval_at(Instant::now(), Duration::from_millis(state.ui_update_rate_ms));
                            }
                        },
                        KeyCode::Tab => {
                            state.next_timer();
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
