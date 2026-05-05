use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use ratatui::layout::Rect;
use std::io::stdout;
 
pub fn enable_mouse() -> Result<(), Box<dyn std::error::Error>> {
    execute!(stdout(), EnableMouseCapture)?;
    Ok(())
}
 
pub fn disable_mouse() -> Result<(), Box<dyn std::error::Error>> {
    execute!(stdout(), DisableMouseCapture)?;
    Ok(())
}
 
pub fn hit_test(col: u16, row: u16, areas: &[Rect]) -> Option<usize> {
    areas.iter().position(|rect| {
        col >= rect.x
            && col < rect.x + rect.width
            && row >= rect.y
            && row < rect.y + rect.height
    })
}
