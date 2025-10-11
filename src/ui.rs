use color_eyre::eyre::Result;
use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Stdout};

use crate::app::App;

/// Gerencia o ciclo de vida do terminal.
pub struct TuiManager {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TuiManager {
    pub fn new() -> Result<Self> {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    /// Entra no modo "raw" e na tela alternativa.
    pub fn init(&mut self) -> Result<()> {
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        self.terminal.clear()?;
        Ok(())
    }

    /// Restaura o terminal para seu estado original.
    pub fn restore(&mut self) -> Result<()> {
        disable_raw_mode()?;
        io::stdout().execute(LeaveAlternateScreen)?;
        self.terminal.show_cursor()?;
        Ok(())
    }

    /// Desenha a UI no terminal.
    pub fn draw(&mut self, app: &mut App) -> Result<()> {
        self.terminal.draw(|frame| crate::tui::ui(frame, app))?;
        Ok(())
    }
}

impl Drop for TuiManager {
    fn drop(&mut self) {
        // Garante que o terminal seja restaurado mesmo em caso de pânico.
        let _ = self.restore();
    }
}
