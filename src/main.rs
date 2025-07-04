use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify::{Event as NotifyEvent, RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},

    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    sync::mpsc,
    time::sleep,
};

#[derive(Parser, Debug)]
#[command(name = "watchhound")]
#[command(about = "A file system watcher that shows git diff information")]
struct Args {
    /// Directory to watch
    directory: PathBuf,
}

#[derive(Debug, Clone)]
struct AppState {
    git_stat: String,
    git_diff: String,
    changed_files: Vec<String>,
    current_file_index: usize,
    scroll_position: u16,
    last_update: Option<chrono::DateTime<Utc>>,
    error_message: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            git_stat: String::new(),
            git_diff: String::new(),
            changed_files: Vec::new(),
            current_file_index: 0,
            scroll_position: 0,
            last_update: None,
            error_message: None,
        }
    }
}

struct App {
    state: Arc<Mutex<AppState>>,
    should_quit: bool,
    directory: PathBuf,
}

impl App {
    fn new(directory: PathBuf) -> Self {
        Self {
            state: Arc::new(Mutex::new(AppState::default())),
            should_quit: false,
            directory,
        }
    }

    fn render(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
            .split(f.size());

        let state = self.state.lock().unwrap();
        
        // Left pane - git stat
        let left_block = Block::default()
            .title("Git Status")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White));

        let git_stat_text = if state.git_stat.is_empty() {
            "No changes detected".to_string()
        } else {
            state.git_stat.clone()
        };

        let git_stat_paragraph = Paragraph::new(git_stat_text)
            .block(left_block)
            .wrap(Wrap { trim: true });

        f.render_widget(git_stat_paragraph, chunks[0]);

        // Right pane - git diff
        let right_title = if !state.changed_files.is_empty() {
            let current_file = &state.changed_files[state.current_file_index];
            format!("Git Diff - {} ({}/{})", current_file, state.current_file_index + 1, state.changed_files.len())
        } else {
            "Git Diff".to_string()
        };

        let right_block = Block::default()
            .title(right_title)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White));

        let git_diff_text = if state.git_diff.is_empty() {
            "No changes to show".to_string()
        } else {
            state.git_diff.clone()
        };

        let git_diff_paragraph = Paragraph::new(git_diff_text)
            .block(right_block)
            .wrap(Wrap { trim: true })
            .scroll((state.scroll_position, 0));

        f.render_widget(git_diff_paragraph, chunks[1]);

        // Show error message if any
        if let Some(error) = &state.error_message {
            let error_area = centered_rect(60, 20, f.size());
            f.render_widget(Clear, error_area);
            let error_block = Block::default()
                .title("Error")
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::Red));
            
            let error_paragraph = Paragraph::new(error.clone())
                .block(error_block)
                .wrap(Wrap { trim: true });
            
            f.render_widget(error_paragraph, error_area);
        }

        // Show controls and last update time
        let controls = "Controls: ←→ Navigate files | Space: Scroll down | q: Quit | r: Refresh";
        let status_line = if let Some(last_update) = &state.last_update {
            format!("{} | Last updated: {}", controls, last_update.format("%H:%M:%S"))
        } else {
            controls.to_string()
        };

        let status_area = Rect {
            x: 0,
            y: f.size().height - 1,
            width: f.size().width,
            height: 1,
        };
        
        let status_paragraph = Paragraph::new(status_line)
            .style(Style::default().fg(Color::Gray));
        
        f.render_widget(status_paragraph, status_area);
    }

    fn navigate_to_previous_file(&self) {
        let mut state = self.state.lock().unwrap();
        if !state.changed_files.is_empty() && state.current_file_index > 0 {
            state.current_file_index -= 1;
            state.scroll_position = 0; // Reset scroll when changing files
        }
    }

    fn navigate_to_next_file(&self) {
        let mut state = self.state.lock().unwrap();
        if !state.changed_files.is_empty() && state.current_file_index < state.changed_files.len() - 1 {
            state.current_file_index += 1;
            state.scroll_position = 0; // Reset scroll when changing files
        }
    }

    fn scroll_down(&self) {
        let mut state = self.state.lock().unwrap();
        state.scroll_position += 1;
    }

    async fn update_current_file_diff(&self) {
        let current_file = {
            let state = self.state.lock().unwrap();
            if state.changed_files.is_empty() {
                return;
            }
            state.changed_files[state.current_file_index].clone()
        };

        let git_diff = match self.run_git_diff_for_file(&current_file).await {
            Ok(output) => output,
            Err(e) => {
                format!("Error getting diff for {}: {}", current_file, e)
            }
        };

        {
            let mut state = self.state.lock().unwrap();
            state.git_diff = git_diff;
        }
    }

    async fn load_initial_state(&self) -> Result<()> {
        // Get initial git diff --stat
        let git_stat = match self.run_git_diff_stat().await {
            Ok(output) => output,
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                state.error_message = Some(format!("Git stat error: {}", e));
                return Ok(());
            }
        };

        // Get all changed files
        let changed_files = match self.get_changed_files().await {
            Ok(files) => files,
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                state.error_message = Some(format!("Error finding changed files: {}", e));
                return Ok(());
            }
        };

        // Update state with initial data
        {
            let mut state = self.state.lock().unwrap();
            state.git_stat = git_stat;
            state.changed_files = changed_files;
            state.current_file_index = 0;
            state.scroll_position = 0;
            state.last_update = Some(Utc::now());
        }

        // Get diff for first file if available
        if !{
            let state = self.state.lock().unwrap();
            state.changed_files.is_empty()
        } {
            self.update_current_file_diff().await;
        }

        Ok(())
    }

    async fn handle_file_change(&self, _path: &Path) -> Result<()> {
        // Wait 5 seconds before processing
        sleep(Duration::from_secs(5)).await;

        // Clear error message
        {
            let mut state = self.state.lock().unwrap();
            state.error_message = None;
        }

        // Run git diff --stat
        let git_stat = match self.run_git_diff_stat().await {
            Ok(output) => output,
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                state.error_message = Some(format!("Git stat error: {}", e));
                return Ok(());
            }
        };

        // Get all changed files
        let changed_files = match self.get_changed_files().await {
            Ok(files) => files,
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                state.error_message = Some(format!("Error finding changed files: {}", e));
                return Ok(());
            }
        };

        // Update state with new files list
        {
            let mut state = self.state.lock().unwrap();
            state.git_stat = git_stat;
            state.changed_files = changed_files;
            // Reset to first file if files changed
            if !state.changed_files.is_empty() {
                state.current_file_index = 0;
                state.scroll_position = 0;
            }
            state.last_update = Some(Utc::now());
        }

        // Get diff for current file
        if !{
            let state = self.state.lock().unwrap();
            state.changed_files.is_empty()
        } {
            self.update_current_file_diff().await;
        }

        Ok(())
    }

    async fn run_git_diff_stat(&self) -> Result<String> {
        let output = Command::new("git")
            .args(["diff", "--stat"])
            .current_dir(&self.directory)
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Git command failed: {}", String::from_utf8_lossy(&output.stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn run_git_diff_for_file(&self, file: &str) -> Result<String> {
        let output = Command::new("git")
            .args(["diff", file])
            .current_dir(&self.directory)
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Git diff failed: {}", String::from_utf8_lossy(&output.stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn get_changed_files(&self) -> Result<Vec<String>> {
        let output = Command::new("git")
            .args(["diff", "--name-only"])
            .current_dir(&self.directory)
            .output()?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Git diff --name-only failed: {}", String::from_utf8_lossy(&output.stderr)));
        }

        let files = String::from_utf8_lossy(&output.stdout);
        let files: Vec<String> = files.trim().lines().map(|s| s.to_string()).collect();
        
        Ok(files)
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

async fn setup_file_watcher(directory: PathBuf, app_state: Arc<Mutex<AppState>>) -> Result<()> {
    let (tx, mut rx) = mpsc::channel(100);
    let mut debounce_map: HashMap<PathBuf, Instant> = HashMap::new();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<NotifyEvent, notify::Error>| {
            if let Ok(event) = res {
                if let Err(e) = tx.blocking_send(event) {
                    eprintln!("Error sending file event: {}", e);
                }
            }
        },
        notify::Config::default(),
    )?;

    watcher.watch(&directory, RecursiveMode::Recursive)?;

    // Create app instance for handling file changes
    let app = App::new(directory);
    let app_state_clone = app_state.clone();

    while let Some(event) = rx.recv().await {
        if let Some(path) = event.paths.first() {
            let path_clone = path.clone();
            let now = Instant::now();
            
            // Debounce: only process if it's been more than 5 seconds since last event for this path
            if let Some(last_time) = debounce_map.get(&path_clone) {
                if now.duration_since(*last_time) < Duration::from_secs(5) {
                    continue;
                }
            }
            
            debounce_map.insert(path_clone.clone(), now);
            
            // Handle the file change
            let mut app_clone = App::new(app.directory.clone());
            app_clone.state = app_state_clone.clone();
            
            tokio::spawn(async move {
                if let Err(e) = app_clone.handle_file_change(&path_clone).await {
                    eprintln!("Error handling file change: {}", e);
                }
            });
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Verify the directory exists and is a git repository
    if !args.directory.exists() {
        return Err(anyhow::anyhow!("Directory does not exist: {:?}", args.directory));
    }

    if !args.directory.join(".git").exists() {
        return Err(anyhow::anyhow!("Directory is not a git repository: {:?}", args.directory));
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(args.directory.clone());
    
    // Load initial state immediately
    if let Err(e) = app.load_initial_state().await {
        eprintln!("Error loading initial state: {}", e);
    }
    
    // Start file watcher in background
    let watcher_state = app.state.clone();
    let watcher_directory = args.directory.clone();
    tokio::spawn(async move {
        if let Err(e) = setup_file_watcher(watcher_directory, watcher_state).await {
            eprintln!("File watcher error: {}", e);
        }
    });

    // Main event loop
    loop {
        terminal.draw(|f| app.render(f))?;

        // Handle input events
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            app.should_quit = true;
                        }
                        KeyCode::Char('r') => {
                            // Manual refresh
                            let mut app_clone = App::new(app.directory.clone());
                            app_clone.state = app.state.clone();
                            tokio::spawn(async move {
                                if let Err(e) = app_clone.load_initial_state().await {
                                    eprintln!("Error during manual refresh: {}", e);
                                }
                            });
                        }
                        KeyCode::Left => {
                            app.navigate_to_previous_file();
                            let mut app_clone = App::new(app.directory.clone());
                            app_clone.state = app.state.clone();
                            tokio::spawn(async move {
                                app_clone.update_current_file_diff().await;
                            });
                        }
                        KeyCode::Right => {
                            app.navigate_to_next_file();
                            let mut app_clone = App::new(app.directory.clone());
                            app_clone.state = app.state.clone();
                            tokio::spawn(async move {
                                app_clone.update_current_file_diff().await;
                            });
                        }
                        KeyCode::Char(' ') => {
                            app.scroll_down();
                        }
                        _ => {}
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
} 