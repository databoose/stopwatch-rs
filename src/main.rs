use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval_at, Duration, Instant, MissedTickBehavior};
use tokio::time;

use crossterm::event::{self, Event, KeyCode};
use ratatui::widgets::{block, Padding};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    prelude::{Alignment, Stylize},
    style::{palette::tailwind::GREEN, Color},
    text::{Line, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

static UI_UPDATE_RATE_MS:u64 = 100;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let mut terminal = ratatui::init();

    let msg:Option<String> = match args.len() {
        2 => Some(args[1].clone()),
        1 => None,
        _ => {
            eprintln!("Invalid input, Usage : {} <message>", args[0]);
            std::process::exit(1);
        }
    };

    let current_time = Arc::new(Mutex::new(Time::new()));
    let counter_time = Arc::clone(&current_time); // points to same memory, we are just cloning the pointer effectively
    tokio::spawn(async move {
        counter(counter_time).await;
    });

    // ui / user event loop
    loop {
        let time_snapshot = {
            let time_guard = current_time.lock().await;
            time_guard.clone()
        };

        terminal
            .draw(|frame| draw(frame, &time_snapshot, &msg))
            .expect("failed to draw frame");

        if crossterm::event::poll(Duration::from_millis(UI_UPDATE_RATE_MS))? {
            if let Event::Key(key) = event::read().expect("failed to read event") {
                match key.code {
                    KeyCode::Char('q') => break,
                    _ => {}
                }
            }
        }
    }

    ratatui::restore();
    Ok(())
}

fn draw(frame: &mut Frame, time: &Time, msg: &Option<String>) {
    let time_block = Block::default()
        .borders(Borders::ALL)
        .padding(Padding::uniform(1));

    let time_display = match msg.as_ref() {
        None => format!("{}d:{}h:{}m:{}s", time.days, time.hour, time.minute, time.second),
        Some(msg) => format!("{}d:{}h:{}m:{}s ({})", time.days, time.hour, time.minute, time.second, msg),
    };

    let time_text = Paragraph::new(time_display)
        .alignment(Alignment::Center)
        .block(time_block);

    frame.render_widget(time_text, frame.area());
}
