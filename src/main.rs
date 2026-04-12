use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::enable_raw_mode;
use ratatui::widgets::Padding;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Alignment,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::time;
use tokio::time::{interval_at, Duration, Instant};

#[derive(Clone, Serialize, Deserialize, Debug)]
struct PersistedTimer {
    timer_id: usize,
    elapsed_seconds: u64,
    last_wall_clock: u64, // UNIX timestamp when last saved
    label: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct PersistedState {
    timers: Vec<PersistedTimer>,
    selected_timer: usize,
    save_timestamp: u64,
}

#[derive(Clone)]
struct Time {
    second: u16,
    minute: u16,
    hour: u16,
    days: u16,
    // rtc-based drift check fields
    total_seconds: u64,
    start_wall_clock: u64, // UNIX timestamp when timer started/resumed
}

impl Time {
    fn new() -> Self {
        let now = Self::current_unix_time();
        Self {
            second: 0,
            minute: 0,
            hour: 0,
            days: 0,
            total_seconds: 0,
            start_wall_clock: now,
        }
    }
    
    // calculate elapsed time since last save using wall-clock
    fn from_persisted(persisted: &PersistedTimer, now: u64) -> Self {
        let elapsed_since_save = now.saturating_sub(persisted.last_wall_clock);
        let total_seconds = persisted.elapsed_seconds + elapsed_since_save;
        let original_start = persisted
            .last_wall_clock
            .saturating_sub(persisted.elapsed_seconds);
        let mut time = Self {
            second: 0,
            minute: 0,
            hour: 0,
            days: 0,
            total_seconds,
            start_wall_clock: original_start,
        };
        time.update_display_fields();
        time
    }
    
    fn current_unix_time() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
    
    fn update_display_fields(&mut self) {
        let total = self.total_seconds;
        self.second = (total % 60) as u16;
        self.minute = ((total / 60) % 60) as u16;
        self.hour = ((total / 3600) % 24) as u16;
        self.days = (total / 86400) as u16;
    }
    
    fn increment(&mut self) {
        self.total_seconds += 1;
        self.update_display_fields();
    }
    
    fn to_persisted(&self, timer_id: usize, label: &Option<String>) -> PersistedTimer {
        PersistedTimer {
            timer_id,
            elapsed_seconds: self.total_seconds,
            last_wall_clock: Self::current_unix_time(),
            label: label.clone(),
        }
    }
}

struct Timer {
    timer_state: Arc<Mutex<Time>>,
    label: Option<String>,
    task_handle: Option<tokio::task::JoinHandle<()>>,
    timer_id: usize,
}

impl Timer {
    fn new(label: Option<String>, timer_id: usize) -> Self {
        Self {
            timer_state: Arc::new(Mutex::new(Time::new())),
            label,
            task_handle: None,
            timer_id,
        }
    }
    fn to_persisted(&self, time_snapshot: &Time) -> PersistedTimer {
        time_snapshot.to_persisted(self.timer_id, &self.label)
    }
}

struct State {
    timers: Vec<Timer>,
    selected_timer: usize,
    ui_update_rate_ms: u64,
    input_mode: bool,
    input_buffer: String,
    show_help: bool,
    next_timer_id: usize, // For assigning unique IDs to new timers
    save_interval_seconds: u64,
    last_save_time: u64,
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
            timers: vec![Timer::new(initial_label, 0)],
            selected_timer: 0,
            ui_update_rate_ms: 27, // 37 fps by default, 20 feels too sluggish imo
            input_mode: false,
            input_buffer: String::new(),
            show_help: true,
            next_timer_id: 1,
            save_interval_seconds: 30,
            last_save_time: Time::current_unix_time(),
        }
    }
    
    async fn reset_timer(&mut self) {
        let mut time_guard = self.timers[self.selected_timer].timer_state.lock().await;
        let now = Time::current_unix_time();
        time_guard.second = 0;
        time_guard.minute = 0;
        time_guard.hour = 0;
        time_guard.days = 0;
        time_guard.total_seconds = 0;
        time_guard.start_wall_clock = now;
    }
    
    fn add_timer(&mut self) {
        if self.timers.len() < 12 {
            let timer_id = self.next_timer_id;
            self.next_timer_id += 1;
            self.timers.push(Timer::new(None, timer_id));
            self.selected_timer = self.timers.len() - 1;
        }
    }
    
    fn remove_timer(&mut self) {
        if self.timers.len() > 1 {
            if let Some(handle) = self.timers[self.selected_timer].task_handle.take() {
                handle.abort();
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
            self.selected_timer = (self.selected_timer + 1) % self.timers.len();
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

    fn get_save_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let exe_path = env::current_exe()?;
        let exe_dir = exe_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        Ok(exe_dir.join("timers.toml"))
    }
    
    async fn save_to_disk(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut persisted_timers = Vec::new();
        for timer in &self.timers {
            let time_guard = timer.timer_state.lock().await;
            persisted_timers.push(timer.to_persisted(&time_guard));
        }
        let state = PersistedState {
            timers: persisted_timers,
            selected_timer: self.selected_timer,
            save_timestamp: Time::current_unix_time(),
        };
        let toml_string = toml::to_string_pretty(&state)?;
        let save_path = Self::get_save_path()?;
        fs::write(save_path, toml_string)?;
        Ok(())
    }
    
    fn load_from_disk() -> Result<Option<PersistedState>, Box<dyn std::error::Error>> {
        let save_path = Self::get_save_path()?;
        if !save_path.exists() {
            return Ok(None);
        }
        let contents = fs::read_to_string(save_path)?;
        let state: PersistedState = toml::from_str(&contents)?;
        Ok(Some(state))
    }
    
    async fn resume_from_persisted(&mut self, persisted: PersistedState) {
        let now = Time::current_unix_time();
        // Clear existing timers
        for timer in &mut self.timers {
            if let Some(handle) = timer.task_handle.take() {
                handle.abort();
            }
        }
        self.timers.clear();
        // Restore timers from persisted state
        for p_timer in persisted.timers {
            let time = Time::from_persisted(&p_timer, now);
            let mut timer = Timer::new(p_timer.label, p_timer.timer_id);
            *timer.timer_state.lock().await = time;
            // Spawn counter task for resumed timer
            let time_counter = Arc::clone(&timer.timer_state);
            let handle = tokio::spawn(async move {
                hybrid_counter(time_counter).await;
            });
            timer.task_handle = Some(handle);
            self.timers.push(timer);
            if p_timer.timer_id >= self.next_timer_id {
                self.next_timer_id = p_timer.timer_id + 1;
            }
        }
        self.selected_timer = persisted
            .selected_timer
            .min(self.timers.len().saturating_sub(1));
        self.last_save_time = now;
    }
    
    fn should_save(&self) -> bool {
        let now = Time::current_unix_time();
        now.saturating_sub(self.last_save_time) >= self.save_interval_seconds
    }
    
    fn mark_saved(&mut self) {
        self.last_save_time = Time::current_unix_time();
    }
}

async fn hybrid_counter(time: Arc<Mutex<Time>>) {
    let mut interval = interval_at(
        Instant::now() + Duration::from_secs(1),
        Duration::from_secs(1),
    );
    loop {
        interval.tick().await;
        let mut time_guard = time.lock().await;
        // increment sleep-based counter (original behavior)
        time_guard.increment();
        if time_guard.total_seconds % 300 == 0 {
            let now = Time::current_unix_time();
            let expected = now.saturating_sub(time_guard.start_wall_clock);
            if expected.abs_diff(time_guard.total_seconds) > 2 {
                eprintln!("drift detected"); // just alert user for now
                // time_guard.total_seconds = expected;
                // time_guard.update_display_fields();
            }
        }
    }
}

fn get_layout_areas(frame: &Frame, timer_count: usize) -> Vec<Rect> {
    let area = frame.area();
    if timer_count <= 1 {
        return vec![area];
    }
    // for 2 we just do a horizontal split, vertical with 2x timers looks wrong imo
    if timer_count == 2 {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        return vec![chunks[0], chunks[1]];
    }
    
    let cols = (timer_count as f64).sqrt().ceil() as usize;
    let rows = timer_count.div_ceil(cols);
    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Ratio(1, rows as u32); rows])
        .split(area);
    
    row_areas
        .iter()
        .enumerate()
        // flatten so we get one vector of multiple Rects rather than multiple individual vecs containing rect
        .flat_map(|(i, &row)| {
            let start = i * cols;
            let row_cols = (timer_count - start).min(cols);
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Ratio(1, row_cols as u32); row_cols])
                .split(row)
                .to_vec()
        })
        .collect()
}

fn draw_timer_box(
    frame: &mut Frame,
    area: Rect,
    timer: &Timer,
    time_snapshot: &Time,
    index: usize,
    state: &State,
) {
    let is_selected = index == state.selected_timer;
    let border_color = if is_selected {
        Color::Green
    } else {
        Color::Gray
    };
    let title = format!(" Timer {} ", index + 1);
    let time_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(title)
        .padding(Padding::uniform(1));
    let time_display = if state.input_mode && is_selected {
        format!("Label: {}_", state.input_buffer)
    } else {
        let time_str = format!(
            "{}d:{}h:{}m:{}s",
            time_snapshot.days, time_snapshot.hour, time_snapshot.minute, time_snapshot.second
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
        Span::styled("Y", Style::default().fg(Color::Green)),
        Span::styled("/", Style::default().fg(Color::Gray)),
        Span::styled("N", Style::default().fg(Color::Red)),
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
    let prompt_area = Rect::new(x_pos, y_pos, rect_width, rect_height);
    
    frame.render_widget(prompt_paragraph, prompt_area);
}

fn draw_help(frame: &mut Frame, update_rate: u64) {
    let area = frame.area();
    let fps = 1000 / update_rate;
    let help_text = vec![
        "Shortcuts:",
        " ctrl + q - Quit",
        " ctrl + a - Add timer (max 8)",
        " ctrl + d - Delete selected timer",
        " ctrl + r - Reset selected timer",
        " tab - Next timer",
        " l - Set label for timer",
        " h - Toggle help",
        " ↑/↓ - Increase/Decrease UI FPS",
        " esc - Cancel input",
    ];
    
    let help_area = Rect {
        x: area.width.saturating_sub(42),
        y: area.height.saturating_sub(17),
        width: (area.width / 4).max(38).min(area.width),
        height: (area.height / 3).max(15).min(area.height),
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


#[tokio::main(worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut terminal = ratatui::init();
    let mut state = State::new();
    // try to load persisted state on startup
    if let Ok(Some(persisted)) = State::load_from_disk() {
        state.resume_from_persisted(persisted).await;
    } else {
        // start fresh timers if no persisted state
        for timer in &mut state.timers {
            let time_counter = Arc::clone(&timer.timer_state);
            let handle = tokio::spawn(async move {
                hybrid_counter(time_counter).await;
            });
            timer.task_handle = Some(handle);
        }
    }
    
    let mut interval = time::interval_at(
        Instant::now(),
        Duration::from_millis(state.ui_update_rate_ms),
    );
    
    'main_loop: loop {
        interval.tick().await;
        // Snapshot timer states for rendering
        let mut time_snapshots = Vec::new();
        for timer in &state.timers {
            let time_guard = timer.timer_state.lock().await;
            time_snapshots.push(time_guard.clone());
        }
        terminal.draw(|frame| {
            let areas = get_layout_areas(frame, state.timers.len());
            for i in 0..state.timers.len() {
                draw_timer_box(
                    frame,
                    areas[i],
                    &state.timers[i],
                    &time_snapshots[i],
                    i,
                    &state,
                );
            }
            if state.show_help {
                draw_help(frame, state.ui_update_rate_ms);
            }
        })?;
        // auto-save periodically
        if state.should_save() {
            if let Err(e) = state.save_to_disk().await {
                eprintln!("Warning: Failed to save state: {}", e);
            } else {
                state.mark_saved();
            }
        }
        // handle input
        if crossterm::event::poll(Duration::ZERO)? {
            if let Event::Key(key) = event::read()? {
                if state.input_mode {
                    match key.code {
                        KeyCode::Enter => state.set_label(),
                        KeyCode::Esc => {
                            state.input_mode = false;
                            state.input_buffer.clear();
                        }
                        KeyCode::Backspace => {
                            state.input_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            state.input_buffer.push(c);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            'confirm_loop: loop {
                                terminal.draw(|frame| {
                                    draw_confirmation_prompt(frame);
                                })?;
                                if let Event::Key(key) = event::read()? {
                                    match key.code {
                                        KeyCode::Char('y') => {
                                            state.save_to_disk().await; // save state and quit
                                            break 'main_loop;
                                        }
                                        KeyCode::Char('n') => break 'confirm_loop,
                                        _ => continue,
                                    }
                                }
                            }
                        }
                        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if state.timers.len() < 12 {
                                state.add_timer();
                                let idx = state.timers.len() - 1;
                                let timer = &mut state.timers[idx];
                                let counter_time = Arc::clone(&timer.timer_state);
                                let handle = tokio::spawn(async move {
                                    hybrid_counter(counter_time).await;
                                });
                                timer.task_handle = Some(handle);
                            }
                        }
                        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            if state.timers.len() > 1 {
                                state.remove_timer();
                            }
                        }
                        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            state.reset_timer().await;
                        }
                        KeyCode::Char('h') => state.toggle_help(),
                        KeyCode::Char('l') => {
                            state.input_mode = true;
                            state.input_buffer.clear();
                        }
                        KeyCode::Up => {
                            if state.ui_update_rate_ms > 10 {
                                state.ui_update_rate_ms = state.ui_update_rate_ms.saturating_sub(5);
                                interval = time::interval_at(
                                    Instant::now(),
                                    Duration::from_millis(state.ui_update_rate_ms),
                                );
                            }
                        }
                        KeyCode::Down => {
                            if state.ui_update_rate_ms < 100 {
                                state.ui_update_rate_ms = state.ui_update_rate_ms.saturating_add(5);
                                interval = time::interval_at(
                                    Instant::now(),
                                    Duration::from_millis(state.ui_update_rate_ms),
                                );
                            }
                        }
                        KeyCode::Tab => state.next_timer(),
                        _ => {}
                    }
                }
            }
        }
    }
    // save on exit
    let _ = state.save_to_disk();
    ratatui::restore();
    Ok(())
}
