//! TUI application state machine.

use std::io::{self, stdout};
use std::path::PathBuf;

use cid::Cid;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tui_tree_widget::TreeState;

use crate::export::{export, ExportFormat, ExportOptions};
use crate::store::AnyStore;
use crate::tree::{NodeId, TreeModel};
use crate::ui;

/// Application state.
pub struct App {
    pub tree: TreeModel,
    pub tree_state: TreeState<NodeId>,
    pub should_quit: bool,
    pub last_error: Option<String>,
    pub export_path: Option<PathBuf>,
}

impl App {
    /// Create a new application.
    pub fn new(
        store: AnyStore,
        root_cid: Cid,
        schema_cid: Cid,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let tree = TreeModel::new(store, root_cid, schema_cid)?;

        // Initialize tree state with root node selected and opened
        let mut tree_state = TreeState::default();
        if let Some(root_id) = tree.roots.first() {
            tree_state.select(vec![root_id.clone()]);
            tree_state.open(vec![root_id.clone()]);
        }

        Ok(Self {
            tree,
            tree_state,
            should_quit: false,
            last_error: None,
            export_path: None,
        })
    }

    /// Run the TUI application.
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        stdout().execute(EnterAlternateScreen)?;

        let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

        let result = self.event_loop(&mut terminal);

        disable_raw_mode()?;
        stdout().execute(LeaveAlternateScreen)?;

        result
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            terminal.draw(|frame| ui::render(frame, self))?;

            if self.should_quit {
                break;
            }

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    self.handle_key(key.code);
                }
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, code: KeyCode) {
        self.last_error = None;

        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.tree_state.key_up();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.tree_state.key_down();
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.tree_state.key_left();
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.tree_state.key_right();
            }
            KeyCode::Enter => {
                self.tree_state.toggle_selected();
            }
            KeyCode::Char('z') => {
                self.zoom_in_selected();
            }
            KeyCode::Char('s') => {
                self.explore_schema();
            }
            KeyCode::Char('b') | KeyCode::Backspace => {
                self.zoom_out();
            }
            KeyCode::Char('e') => {
                self.export_current(ExportFormat::Json);
            }
            KeyCode::Char('y') => {
                self.export_current(ExportFormat::Yaml);
            }
            _ => {}
        }
    }

    fn zoom_in_selected(&mut self) {
        let node_id = match self.tree_state.selected().last() {
            Some(id) => id.clone(),
            None => return,
        };
        match self.tree.zoom_in(&node_id) {
            Ok(true) => self.reset_tree_state(),
            Ok(false) => {}
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    fn zoom_out(&mut self) {
        match self.tree.zoom_out() {
            Ok(true) => self.reset_tree_state(),
            Ok(false) => {}
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    fn explore_schema(&mut self) {
        let node_id = match self.tree_state.selected().last() {
            Some(id) => id.clone(),
            None => return,
        };
        match self.tree.zoom_to_schema(&node_id) {
            Ok(true) => self.reset_tree_state(),
            Ok(false) => {}
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    fn reset_tree_state(&mut self) {
        self.tree_state = TreeState::default();
        if let Some(root_id) = self.tree.roots.first() {
            self.tree_state.select(vec![root_id.clone()]);
            self.tree_state.open(vec![root_id.clone()]);
        }
    }

    fn export_current(&mut self, format: ExportFormat) {
        // Get selected node
        let node_id = match self.tree_state.selected().last() {
            Some(id) => id.clone(),
            None => return,
        };
        let node = match self.tree.get_node(&node_id) {
            Some(n) => n,
            None => return,
        };

        let options = ExportOptions::default();
        let ext = match format {
            ExportFormat::Json => "json",
            ExportFormat::Yaml => "yaml",
        };

        // Determine what to export: for bonds use the linked CID, otherwise use root
        let (cid, schema_cid) = if let Some(bond_cid) = node.cid {
            (bond_cid, node.schema_cid)
        } else {
            // Non-bond node: fall back to current root
            (self.tree.root_cid(), self.tree.root_schema_cid())
        };

        let filename = format!("export_{}.{}", &cid.to_string()[..12], ext);

        match export(
            self.tree.store(),
            self.tree.schemas(),
            cid,
            schema_cid,
            format,
            &options,
        ) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&filename, content) {
                    self.last_error = Some(format!("Write error: {}", e));
                } else {
                    self.export_path = Some(PathBuf::from(&filename));
                }
            }
            Err(e) => {
                self.last_error = Some(format!("Export error: {}", e));
            }
        }
    }
}
