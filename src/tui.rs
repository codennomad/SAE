use color_eyre::eyre::Result;
use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use std::io::{self, Stdout};
use crate::{app::App, ui::ui};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

pub struct TuiManager {
    terminal: Option<Terminal<CrosstermBackend<Stdout>>>,
}

impl TuiManager {
    pub fn new() -> Result<Self> {
        Ok(Self {
            terminal: None,
        })
    }

    pub fn init(&mut self) -> Result<()> {
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        
        self.terminal = Some(terminal);
        Ok(())
    }

    pub fn restore(&mut self) -> Result<()> {
        if let Some(mut terminal) = self.terminal.take() {
            disable_raw_mode()?;
            crossterm::execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen
            )?;
            terminal.show_cursor()?;
        }
        Ok(())
    }

    pub fn draw(&mut self, app: &mut App) -> Result<()> {
        if let Some(terminal) = &mut self.terminal {
            terminal.draw(|frame| ui(frame, app))?;
        }
        Ok(())
    }

    pub fn size(&self) -> Result<(u16, u16)> {
        if let Some(terminal) = &self.terminal {
            let size = terminal.size()?;
            Ok((size.width, size.height))
        } else {
            Ok((80, 24)) // Default size
        }
    }
}

impl Drop for TuiManager {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}