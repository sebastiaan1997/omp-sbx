use std::io::{self, IsTerminal, Read, Write};

use anyhow::{Context, Result};
use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};

pub fn stderr_is_terminal() -> bool { io::stderr().is_terminal() }
pub fn stdin_is_terminal() -> bool { io::stdin().is_terminal() }

#[cfg(unix)]
fn tty() -> Result<std::fs::File> {
    std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty").context("open /dev/tty")
}

#[cfg(windows)]
fn tty() -> Result<std::fs::File> {
    std::fs::OpenOptions::new().read(true).write(true).open("CONIN$").context("open console input")
}

pub fn read_line(prompt: &str) -> Result<String> {
    if !stdin_is_terminal() { return Ok(String::new()); }
    eprint!("{prompt}");
    io::stderr().flush()?;
    let mut input = tty()?;
    let mut answer = String::new();
    let mut byte = [0_u8; 1];
    while input.read(&mut byte)? == 1 {
        if matches!(byte[0], b'\n' | b'\r') { break; }
        answer.push(byte[0] as char);
    }
    Ok(answer.trim().to_owned())
}

pub fn confirm(prompt: &str) -> Result<bool> {
    if !stdin_is_terminal() { return Ok(false); }
    let mut input = tty()?;
    eprint!("{prompt}");
    io::stderr().flush()?;
    let mut answer = String::new();
    let mut byte = [0_u8; 1];
    while input.read(&mut byte)? == 1 {
        if matches!(byte[0], b'\n' | b'\r') { break; }
        answer.push(byte[0] as char);
    }
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuDecision { Continue, Select(usize), Cancel }

fn handle_key(selected: &mut usize, default: usize, len: usize, optional: bool, code: KeyCode) -> MenuDecision {
    match code {
        KeyCode::Up | KeyCode::Char('k') => { *selected = (*selected + len - 1) % len; MenuDecision::Continue }
        KeyCode::Down | KeyCode::Char('j') => { *selected = (*selected + 1) % len; MenuDecision::Continue }
        KeyCode::Enter => MenuDecision::Select(*selected),
        KeyCode::Char('d') => MenuDecision::Select(default),
        KeyCode::Char('s') if optional => MenuDecision::Cancel,
        KeyCode::Char('q') | KeyCode::Esc => MenuDecision::Cancel,
        _ => MenuDecision::Continue,
    }
}

fn draw_menu(frame: &mut ratatui::Frame<'_>, label: &str, options: &[String], state: &mut ListState, optional: bool) {
    let layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Min(1), Constraint::Length(1)]).split(frame.area());
    let items = options.iter().map(|option| ListItem::new(option.as_str())).collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(label))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, layout[0], state);
    let skip = if optional { " · s skip" } else { "" };
    frame.render_widget(Paragraph::new(format!("↑/↓ or j/k move · Enter select · d default{skip} · q/Esc cancel")), layout[1]);
}

struct MenuCleanup { terminal: Option<Terminal<CrosstermBackend<std::fs::File>>>, raw: bool }

impl Drop for MenuCleanup {
    fn drop(&mut self) {
        if let Some(terminal) = self.terminal.as_mut() {
            let _ = execute!(terminal.backend_mut(), Show, LeaveAlternateScreen);
        }
        if self.raw { let _ = disable_raw_mode(); }
    }
}

pub fn select_menu(label: &str, options: &[String], default: usize, optional: bool) -> Result<Option<usize>> {
    if options.is_empty() { anyhow::bail!("interactive menu requires at least one option"); }
    if !stdin_is_terminal() { anyhow::bail!("interactive menu requires a terminal"); }
    let output = tty()?;
    let terminal = Terminal::new(CrosstermBackend::new(output))?;
    enable_raw_mode()?;
    let mut cleanup = MenuCleanup { terminal: Some(terminal), raw: true };
    execute!(cleanup.terminal.as_mut().expect("menu terminal exists").backend_mut(), EnterAlternateScreen, Hide)?;
    let default = default.min(options.len() - 1);
    let mut selected = default;
    let mut state = ListState::default().with_selected(Some(selected));
    let result = loop {
        cleanup.terminal.as_mut().expect("menu terminal exists").draw(|frame| draw_menu(frame, label, options, &mut state, optional))?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match handle_key(&mut selected, default, options.len(), optional, key.code) {
                MenuDecision::Continue => state.select(Some(selected)),
                MenuDecision::Select(index) => break Some(index),
                MenuDecision::Cancel => break None,
            },
            Event::Resize(_, _) => {}
            _ => {}
        }
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{draw_menu, handle_key, MenuDecision};
    use crossterm::event::KeyCode;
    use ratatui::{backend::TestBackend, buffer::Buffer, widgets::ListState, Terminal};

    #[test]
    fn navigation_wraps_and_default_selects() {
        let mut selected = 0;
        assert_eq!(handle_key(&mut selected, 2, 3, false, KeyCode::Up), MenuDecision::Continue);
        assert_eq!(selected, 2);
        assert_eq!(handle_key(&mut selected, 2, 3, false, KeyCode::Char('j')), MenuDecision::Continue);
        assert_eq!(selected, 0);
        assert_eq!(handle_key(&mut selected, 2, 3, false, KeyCode::Char('d')), MenuDecision::Select(2));
    }

    #[test]
    fn optional_skip_and_required_skip_are_distinct() {
        let mut selected = 0;
        assert_eq!(handle_key(&mut selected, 0, 1, true, KeyCode::Char('s')), MenuDecision::Cancel);
        assert_eq!(handle_key(&mut selected, 0, 1, false, KeyCode::Char('s')), MenuDecision::Continue);
        assert_eq!(handle_key(&mut selected, 0, 1, false, KeyCode::Esc), MenuDecision::Cancel);
    }

    #[test]
    fn render_contains_title_instructions_and_options() {
        let backend = TestBackend::new(50, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let options = vec!["first".to_owned(), "second".to_owned()];
        let mut state = ListState::default().with_selected(Some(1));
        terminal.draw(|frame| draw_menu(frame, "Choose", &options, &mut state, true)).unwrap();
        let buffer: &Buffer = terminal.backend().buffer();
        let text = buffer.content.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(text.contains("Choose"));
        assert!(text.contains("first"));
        assert!(text.contains("second"));
        assert!(text.contains("Enter select"));
    }
}
pub fn pause_key(prompt: &str) -> Result<()> {
    if !stdin_is_terminal() { return Ok(()); }
    eprint!("{prompt}");
    io::stderr().flush()?;
    let mut input = tty()?;
    crossterm::terminal::enable_raw_mode()?;
    let mut byte = [0_u8; 1];
    let result = input.read_exact(&mut byte);
    crossterm::terminal::disable_raw_mode()?;
    eprintln!();
    result.map_err(Into::into)
}
