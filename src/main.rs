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
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    collections::HashMap,
    fs,
    io,
    path::{Path, PathBuf},
    process::{Command, exit},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};
use tokio::{
    sync::mpsc,
    time::sleep,
};

#[derive(Parser, Debug)]
#[command(name = "watchhound")]
#[command(about = "A file system watcher that shows git diff information with colors")]
#[command(long_about = "WatchHound monitors a git repository for changes and displays colorized diffs in real-time.
Features:
- Colorized git diff display (green for additions, red for deletions)
- Highlights recently changed files (within 1 minute) 
- Split-pane interface with git status and detailed diffs
- Navigation between changed files with arrow keys
- Real-time file system monitoring with automatic updates
- Stores diff history and auto-scrolls to new changes
- Diff history management with clear functionality

Controls:
- Left/Right: Navigate between changed files
- Up/Down: Scroll up/down through diffs (5 lines at a time)
- Space: Scroll down through diffs (1 line at a time)
- 'r': Manual refresh
- 'c': Clear diff history
- 'h': Toggle history view (current file vs accumulated history)
- 'q' or Esc: Quit")]
struct Args {
    /// Directory to watch (defaults to current directory). Must be a git repository.
    #[arg(default_value = ".")]
    directory: PathBuf,
}

#[derive(Debug, Clone)]
struct FileInfo {
    path: String,
    last_modified: SystemTime,
}

#[derive(Debug, Clone)]
struct DiffEntry {
    timestamp: chrono::DateTime<chrono::Utc>,
    diff_content: String,
    file_name: String,
    previous_diff: Option<String>,
}

#[derive(Debug, Clone)]
struct AppState {
    git_stat: String,
    git_diff: String,
    changed_files: Vec<String>,
    file_info: HashMap<String, FileInfo>,
    current_file_index: usize,
    scroll_position: u16,
    last_update: Option<chrono::DateTime<Utc>>,
    error_message: Option<String>,
    diff_history: Vec<DiffEntry>,
    show_history: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            git_stat: "WatchHound\n\nStarting up...".to_string(),
            git_diff: "Welcome to WatchHound!\n\nInitializing git repository monitoring...\n\nThis will show colorized git diffs in real-time.\nDiff history is automatically stored and scrolls to new changes.\n\nPress 'h' to toggle history view, 'c' to clear history, 'r' to refresh, 'q' to quit.".to_string(),
            changed_files: Vec::new(),
            file_info: HashMap::new(),
            current_file_index: 0,
            scroll_position: 0,
            last_update: None,
            error_message: None,
            diff_history: Vec::new(),
            show_history: false,
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

    fn parse_diff_line(line: &str) -> Line<'static> {
        let spans = if line.starts_with("@@") {
            // Context header (cyan)
            vec![Span::styled(line.to_string(), Style::default().fg(Color::Cyan))]
        } else if line.starts_with("+++") || line.starts_with("---") {
            // File headers (white/gray)
            vec![Span::styled(line.to_string(), Style::default().fg(Color::Gray))]
        } else if line.starts_with('+') {
            // Added lines (green)
            vec![Span::styled(line.to_string(), Style::default().fg(Color::Green))]
        } else if line.starts_with('-') {
            // Removed lines (red)
            vec![Span::styled(line.to_string(), Style::default().fg(Color::Red))]
        } else if line.starts_with("index ") || line.starts_with("diff --git") {
            // Git metadata (gray)
            vec![Span::styled(line.to_string(), Style::default().fg(Color::Gray))]
        } else {
            // Context lines (white)
            vec![Span::styled(line.to_string(), Style::default().fg(Color::White))]
        };
        
        Line::from(spans)
    }

    fn format_diff_text(diff_text: &str) -> Text<'static> {
        let lines: Vec<Line> = diff_text
            .lines()
            .map(|line| Self::parse_diff_line(line))
            .collect();
        
        Text::from(lines)
    }

    fn format_git_stat_with_status(git_stat: &str, file_mod_status: &HashMap<String, bool>) -> Text<'static> {
        let lines: Vec<Line> = git_stat
            .lines()
            .map(|line| {
                if line.contains("|") {
                    // File change lines with stats
                    let parts: Vec<&str> = line.split('|').collect();
                    if parts.len() >= 2 {
                        let file_part = parts[0].trim().to_string();
                        let stats_part = parts[1].trim().to_string();
                        
                        // Check if file was recently modified (within 1 minute)
                        let is_recent = file_mod_status.get(&file_part).unwrap_or(&false);
                        let file_color = if *is_recent { Color::Yellow } else { Color::White };
                        
                        let mut spans = vec![
                            Span::styled(file_part, Style::default().fg(file_color)),
                            Span::styled(" | ".to_string(), Style::default().fg(Color::Gray)),
                        ];
                        
                        // Color the stats part
                        if stats_part.contains('+') && stats_part.contains('-') {
                            spans.push(Span::styled(stats_part, Style::default().fg(Color::Yellow)));
                        } else if stats_part.contains('+') {
                            spans.push(Span::styled(stats_part, Style::default().fg(Color::Green)));
                        } else if stats_part.contains('-') {
                            spans.push(Span::styled(stats_part, Style::default().fg(Color::Red)));
                        } else {
                            spans.push(Span::styled(stats_part, Style::default().fg(Color::White)));
                        }
                        
                        Line::from(spans)
                    } else {
                        Line::from(vec![Span::styled(line.to_string(), Style::default().fg(Color::White))])
                    }
                } else if line.contains("changed") || line.contains("insertion") || line.contains("deletion") {
                    // Summary line
                    Line::from(vec![Span::styled(line.to_string(), Style::default().fg(Color::Cyan))])
                } else {
                    Line::from(vec![Span::styled(line.to_string(), Style::default().fg(Color::White))])
                }
            })
            .collect();
        
        Text::from(lines)
    }

    fn render(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
            .split(f.size());

        let state = self.state.lock().unwrap();
        
        // Pre-compute file modification status to avoid deadlock
        let file_mod_status: HashMap<String, bool> = state.file_info.iter()
            .map(|(path, info)| {
                let is_recent = if let Ok(elapsed) = info.last_modified.elapsed() {
                    elapsed < Duration::from_secs(60)
                } else {
                    false
                };
                (path.clone(), is_recent)
            })
            .collect();
        
        // Left pane - git stat
        let left_block = Block::default()
            .title("Git Status")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White));

        let git_stat_text = if state.git_stat.is_empty() {
            Text::from("No changes detected")
        } else {
            Self::format_git_stat_with_status(&state.git_stat, &file_mod_status)
        };

        let git_stat_paragraph = Paragraph::new(git_stat_text)
            .block(left_block)
            .wrap(Wrap { trim: true });

        f.render_widget(git_stat_paragraph, chunks[0]);

        // Right pane - git diff
        let right_title = if !state.changed_files.is_empty() {
            let current_file = &state.changed_files[state.current_file_index];
            let is_recent = file_mod_status.get(current_file).unwrap_or(&false);
            let indicator = if *is_recent { " [RECENT]" } else { "" };
            format!("Git Diff - {}{} ({}/{})", current_file, indicator, state.current_file_index + 1, state.changed_files.len())
        } else {
            "Git Diff".to_string()
        };

        let right_block = Block::default()
            .title(right_title)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White));

        let git_diff_text = if state.git_diff.is_empty() {
            Text::from("No changes to show")
        } else {
            Self::format_diff_text(&state.git_diff)
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
        let controls = "Controls: Left/Right: Navigate files | Space: Scroll down | q: Quit | r: Refresh | [RECENT] = Recently changed";
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

    fn scroll_up(&self) {
        let mut state = self.state.lock().unwrap();
        state.scroll_position = state.scroll_position.saturating_sub(1);
    }

    fn scroll_down_fast(&self) {
        let mut state = self.state.lock().unwrap();
        state.scroll_position += 5;
    }

    fn scroll_up_fast(&self) {
        let mut state = self.state.lock().unwrap();
        state.scroll_position = state.scroll_position.saturating_sub(5);
    }

    fn add_diff_to_history(&self, diff_content: String, file_name: String) {
        let mut state = self.state.lock().unwrap();
        
        // Find the previous diff for this file
        let previous_diff = state.diff_history.iter()
            .rev()
            .find(|entry| entry.file_name == file_name)
            .map(|entry| entry.diff_content.clone());
        
        // Add the new diff entry
        let diff_entry = DiffEntry {
            timestamp: chrono::Utc::now(),
            diff_content,
            file_name,
            previous_diff,
        };
        
        state.diff_history.push(diff_entry);
        
        // Keep only the last 50 diff entries to prevent memory issues
        if state.diff_history.len() > 50 {
            state.diff_history.remove(0);
        }
    }

    fn find_first_diff_line(&self, current_diff: &str, previous_diff: &str) -> u16 {
        let current_lines: Vec<&str> = current_diff.lines().collect();
        let previous_lines: Vec<&str> = previous_diff.lines().collect();
        
        // Find the first line that's different between current and previous diff
        let mut first_different_line = None;
        for (i, current_line) in current_lines.iter().enumerate() {
            if i >= previous_lines.len() || current_line != &previous_lines[i] {
                first_different_line = Some(i);
                break;
            }
        }
        
        if let Some(diff_start) = first_different_line {
            // Now find the first actual content change (+ or - line) starting from the different line
            for (i, line) in current_lines.iter().enumerate().skip(diff_start) {
                if line.starts_with('+') && !line.starts_with("+++") {
                    // Found the first addition, scroll to show it with context
                    return (i as u16).saturating_sub(3);
                } else if line.starts_with('-') && !line.starts_with("---") {
                    // Found the first deletion, scroll to show it with context
                    return (i as u16).saturating_sub(3);
                } else if line.starts_with("@@") {
                    // Found a hunk header, look for content changes after it
                    for (j, content_line) in current_lines.iter().enumerate().skip(i + 1) {
                        if content_line.starts_with('+') && !content_line.starts_with("+++") {
                            return (j as u16).saturating_sub(3);
                        } else if content_line.starts_with('-') && !content_line.starts_with("---") {
                            return (j as u16).saturating_sub(3);
                        }
                    }
                }
            }
            
            // If no content changes found, just scroll to the first different line
            return (diff_start as u16).saturating_sub(2);
        }
        
        // If we get here, current diff is same as previous (shouldn't happen)
        // Fall back to smart scroll
        self.calculate_smart_scroll_position(current_diff)
    }



    fn build_accumulated_diff(&self) -> String {
        let state = self.state.lock().unwrap();
        let mut accumulated = String::new();
        
        for (i, entry) in state.diff_history.iter().enumerate() {
            if i > 0 {
                accumulated.push_str("\n\n");
                accumulated.push_str(&format!("=== Update {} at {} ===", i + 1, entry.timestamp.format("%H:%M:%S")));
                accumulated.push_str(&format!(" (File: {}) ===\n", entry.file_name));
            }
            accumulated.push_str(&entry.diff_content);
        }
        
        accumulated
    }

    fn calculate_scroll_position_for_new_diff(&self) -> u16 {
        let state = self.state.lock().unwrap();
        if state.diff_history.len() <= 1 {
            return 0;
        }

        // Calculate lines in all previous diffs
        let mut lines_count = 0u16;
        for (i, entry) in state.diff_history.iter().enumerate() {
            if i == state.diff_history.len() - 1 {
                break; // Don't count the last (new) entry
            }
            
            if i > 0 {
                lines_count += 3; // For separator lines
            }
            lines_count += entry.diff_content.lines().count() as u16;
        }
        
        lines_count
    }

    fn auto_scroll_to_new_diff(&self) {
        let scroll_position = self.calculate_scroll_position_for_new_diff();
        let mut state = self.state.lock().unwrap();
        state.scroll_position = scroll_position;
    }

    fn calculate_smart_scroll_position(&self, diff_content: &str) -> u16 {
        let lines: Vec<&str> = diff_content.lines().collect();
        let mut first_addition_line = None;
        
        // Find the first line that contains an addition (starts with +)
        for (i, line) in lines.iter().enumerate() {
            if line.starts_with('+') && !line.starts_with("+++") {
                first_addition_line = Some(i as u16);
                break; // Found the first addition, stop looking
            }
        }
        
        // If we found additions, scroll to show them (with some context)
        if let Some(first_line) = first_addition_line {
            // Show the addition with some context lines before it
            first_line.saturating_sub(3)
        } else {
            // If no additions found, look for the end of the diff
            let total_lines = lines.len() as u16;
            if total_lines > 10 {
                total_lines.saturating_sub(8)
            } else {
                0
            }
        }
    }

    fn clear_diff_history(&self) {
        let mut state = self.state.lock().unwrap();
        state.diff_history.clear();
        state.git_diff = "Diff history cleared.\n\nMake changes to files to see new diffs here.".to_string();
        state.scroll_position = 0;
    }

    fn toggle_history_view(&self) {
        let mut state = self.state.lock().unwrap();
        state.show_history = !state.show_history;
        state.scroll_position = 0;
    }

    async fn refresh_display(&self) {
        let show_history = {
            let state = self.state.lock().unwrap();
            state.show_history
        };

        if show_history {
            // Show accumulated history
            let accumulated_diff = self.build_accumulated_diff();
            let mut state = self.state.lock().unwrap();
            state.git_diff = if accumulated_diff.is_empty() {
                "No diff history available.\n\nMake changes to files to see diffs here.\n\nPress 'h' to toggle back to current file view.".to_string()
            } else {
                accumulated_diff
            };
        } else {
            // Show current file diff only
            self.update_current_file_diff().await;
        }
    }

    async fn update_current_file_diff(&self) {
        self.update_current_file_diff_internal(false).await;
    }

    async fn update_current_file_diff_with_history(&self) {
        self.update_current_file_diff_internal(true).await;
    }

    async fn update_current_file_diff_internal(&self, store_in_history: bool) {
        let current_file = {
            let state = self.state.lock().unwrap();
            if state.changed_files.is_empty() {
                return;
            }
            state.changed_files[state.current_file_index].clone()
        };

        // Show loading state (but don't store this in history)
        {
            let mut state = self.state.lock().unwrap();
            state.git_diff = format!("Loading diff for {}...", current_file);
        }

        // Brief delay to show loading state
        sleep(Duration::from_millis(100)).await;

        let git_diff = match self.run_git_diff_for_file(&current_file).await {
            Ok(output) => {
                if output.trim().is_empty() {
                    format!("No changes in {}\n\nThis file may have been staged or the changes may be minimal.", current_file)
                } else {
                    output
                }
            },
            Err(e) => {
                format!("Error getting diff for {}: {}\n\nTry refreshing with 'r' or check if the file still exists.", current_file, e)
            }
        };

        if store_in_history {
            // Find the previous diff for this file to compare against
            let previous_diff = {
                let state = self.state.lock().unwrap();
                state.diff_history.iter()
                    .rev()
                    .find(|entry| entry.file_name == current_file)
                    .map(|entry| entry.diff_content.clone())
            };
            
            // Calculate scroll position based on what's actually new
            let scroll_position = if let Some(ref prev_diff) = previous_diff {
                // Find the first line that's different from the previous diff
                self.find_first_diff_line(&git_diff, prev_diff)
            } else {
                // No previous diff, use smart scrolling to find first addition
                self.calculate_smart_scroll_position(&git_diff)
            };
            
            // Store the diff in history
            self.add_diff_to_history(git_diff.clone(), current_file.clone());
            
            // Check if we should show history or current file
            let show_history = {
                let state = self.state.lock().unwrap();
                state.show_history
            };
            
            if show_history {
                // Build and set the accumulated diff with auto-scroll for history view
                self.auto_scroll_to_new_diff();
                let accumulated_diff = self.build_accumulated_diff();
                {
                    let mut state = self.state.lock().unwrap();
                    state.git_diff = accumulated_diff;
                }
            } else {
                // Show current file diff and scroll to show the first different line
                {
                    let mut state = self.state.lock().unwrap();
                    state.git_diff = git_diff.clone();
                    // Use the calculated scroll position to show the first different line
                    state.scroll_position = scroll_position;
                }
            }
        } else {
            // Just show the current diff without storing in history
            {
                let mut state = self.state.lock().unwrap();
                state.git_diff = git_diff;
            }
        }
    }

    async fn load_initial_state(&self) -> Result<()> {
        // Set initial loading state
        {
            let mut state = self.state.lock().unwrap();
            state.git_stat = "WatchHound starting up...\nLoading git status...".to_string();
            state.git_diff = "Initializing git repository scan...\n\nChecking for changes...".to_string();
        }

        // Brief delay to show loading state
        sleep(Duration::from_millis(500)).await;

        // Get initial git diff --stat
        let git_stat = match self.run_git_diff_stat().await {
            Ok(output) => {
                if output.trim().is_empty() {
                    "No changes detected in the repository.\n\nMake some changes to files to see diffs here!\n\nTip: Edit a file and the changes will appear automatically.".to_string()
                } else {
                    output
                }
            },
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to get git status: {}", e));
            }
        };

        // Get all changed files
        let changed_files = match self.get_changed_files().await {
            Ok(files) => files,
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to get changed files: {}", e));
            }
        };

        // Update state with initial data
        {
            let mut state = self.state.lock().unwrap();
            state.git_stat = git_stat;
            state.changed_files = changed_files;
            state.current_file_index = 0;
            state.scroll_position = 0;
            state.last_update = Some(chrono::Utc::now());
            state.error_message = None;
        }

        // Get diff for first file if available
        let has_files = {
            let state = self.state.lock().unwrap();
            !state.changed_files.is_empty()
        };

        if has_files {
            self.update_current_file_diff_with_history().await;
        } else {
            // No files to show diff for
            let no_changes_message = "No changes to display.\n\nTo see colorized diffs:\n1. Make changes to files\n2. Use 'r' to refresh\n3. Use Left/Right to navigate files\n4. Use Space to scroll\n5. Use 'h' to toggle history view\n6. Use 'c' to clear diff history\n\nRecently changed files will be highlighted!\nDiff history is automatically stored and scrolls to new changes.".to_string();
            
            // Store the initial message in history
            self.add_diff_to_history(no_changes_message.clone(), "Initial State".to_string());
            
            let mut state = self.state.lock().unwrap();
            state.git_diff = no_changes_message;
        }

        Ok(())
    }

    async fn handle_file_change(&self, path: &Path) -> Result<()> {
        // Wait 1 second before processing
        sleep(Duration::from_secs(1)).await;

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
            
            // Find the index of the changed file to display it
            let changed_file_path = path.to_string_lossy().to_string();
            let mut new_index = 0;
            
            if !changed_files.is_empty() {
                // Try to find the exact file that changed
                if let Some(index) = changed_files.iter().position(|f| f == &changed_file_path) {
                    new_index = index;
                } else {
                    // If exact match not found, try to find files that end with the same relative path
                    // This handles cases where the watcher might give absolute paths but git gives relative paths
                    let changed_file_name = path.file_name().unwrap_or_default().to_string_lossy();
                    if let Some(index) = changed_files.iter().position(|f| {
                        Path::new(f).file_name().unwrap_or_default().to_string_lossy() == changed_file_name
                    }) {
                        new_index = index;
                    }
                    // If still not found, default to 0 (first file)
                }
                
                state.current_file_index = new_index;
                // Don't reset scroll position here - let auto-scroll handle it
            }
            
            state.changed_files = changed_files;
            state.last_update = Some(Utc::now());
        }

        // Get diff for current file - store in history since this is a real file change
        if !{
            let state = self.state.lock().unwrap();
            state.changed_files.is_empty()
        } {
            self.update_current_file_diff_with_history().await;
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
        
        // Update file modification times
        self.update_file_times(&files);
        
        Ok(files)
    }

    fn update_file_times(&self, files: &[String]) {
        let mut state = self.state.lock().unwrap();
        
        for file in files {
            let file_path = self.directory.join(file);
            if let Ok(metadata) = fs::metadata(&file_path) {
                if let Ok(modified) = metadata.modified() {
                    let file_info = FileInfo {
                        path: file.clone(),
                        last_modified: modified,
                    };
                    state.file_info.insert(file.clone(), file_info);
                }
            }
        }
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
            
            // Debounce: only process if it's been more than 1 second since last event for this path
            if let Some(last_time) = debounce_map.get(&path_clone) {
                if now.duration_since(*last_time) < Duration::from_secs(1) {
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
    
    // Verify the directory exists and is a git repository BEFORE setting up terminal
    if !args.directory.exists() {
        eprintln!("Error: Directory does not exist: {:?}", args.directory);
        eprintln!("Please specify a valid directory path.");
        eprintln!("   Example: watchhound /path/to/your/git/repo");
        eprintln!("   Or run from within a git repository: watchhound");
        exit(1);
    }

    if !args.directory.join(".git").exists() {
        eprintln!("Error: Directory is not a git repository: {:?}", args.directory);
        eprintln!("Please navigate to a git repository or initialize one:");
        eprintln!("   git init");
        eprintln!("   git add .");
        eprintln!("   git commit -m \"Initial commit\"");
        exit(1);
    }

    // Setup terminal (only after validation)
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Set up panic handler to restore terminal
    std::panic::set_hook(Box::new(|_info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
        eprintln!("Application panicked! Terminal has been restored.");
        eprintln!("Please report this issue if it persists.");
    }));

    // Create app
    let mut app = App::new(args.directory.clone());
    
    // Load initial state immediately
    if let Err(e) = app.load_initial_state().await {
        // Restore terminal before showing error
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        
        eprintln!("Error loading initial state: {}", e);
        eprintln!("Make sure you're in a git repository with some changes.");
        eprintln!("   Try making a change to a file and run again.");
        exit(1);
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
    let result = async {
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
                            KeyCode::Up => {
                                app.scroll_up_fast();
                            }
                            KeyCode::Down => {
                                app.scroll_down_fast();
                            }
                            KeyCode::Char('c') => {
                                // Clear diff history
                                app.clear_diff_history();
                            }
                            KeyCode::Char('h') => {
                                // Toggle history view
                                app.toggle_history_view();
                                let mut app_clone = App::new(app.directory.clone());
                                app_clone.state = app.state.clone();
                                tokio::spawn(async move {
                                    app_clone.refresh_display().await;
                                });
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
        Ok::<(), anyhow::Error>(())
    }.await;

    // Always restore terminal, regardless of how we exit
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Handle any errors that occurred during the main loop
    if let Err(e) = result {
        eprintln!("Application error: {}", e);
        eprintln!("Terminal has been restored. Please report this issue if it persists.");
        exit(1);
    }

    Ok(())
} 