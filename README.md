# WatchHound 🐕

A full-screen terminal application that watches a directory for file system changes and displays git diff information in real-time.

## Features

- **Immediate Loading**: Shows current git diff snapshot immediately on startup
- **File System Monitoring**: Watches a specified directory for file changes
- **Git Integration**: Automatically runs `git diff --stat` and shows detailed diffs
- **Split-Pane Interface**: Left pane shows git status, right pane shows diff for current file
- **File Navigation**: Use left/right arrow keys to navigate between changed files
- **Diff Scrolling**: Use space bar to scroll through long diffs
- **Debouncing**: Waits 5 seconds after file changes before updating (prevents excessive git operations)
- **Cross-Platform**: Works on Windows, macOS, and Linux
- **Real-Time Updates**: Updates automatically when files change
- **Manual Refresh**: Press 'r' to manually refresh the view

## Requirements

- Rust 1.70 or later
- Git installed and accessible from command line
- A git repository to monitor

## Installation

1. Clone this repository or create the project:
```bash
git clone <repository-url>
cd watchhound
```

2. Build the project:
```bash
cargo build --release
```

3. Run the application:
```bash
cargo run -- /path/to/your/git/repository
```

## Usage

```bash
watchhound <directory>
```

Where `<directory>` is the path to a git repository you want to monitor.

### Example

```bash
# Watch the current directory
watchhound .

# Watch a specific directory
watchhound /path/to/my/project

# On Windows
watchhound C:\path\to\my\project
```

## Controls

- **q** or **Esc**: Quit the application
- **r**: Manually refresh the git status
- **← →** (Left/Right arrows): Navigate between changed files
- **Space**: Scroll down the current diff

## Interface

The application displays a split-screen interface:

- **Left Pane**: Shows the output of `git diff --stat` with a summary of changed files
- **Right Pane**: Shows the detailed `git diff` for the current file (with file navigation indicator)
- **Status Bar**: Shows navigation controls and last update time at the bottom of the screen
- **Error Messages**: Displays any git or file system errors in a popup

## How It Works

1. **Immediate Loading**: Loads current git diff state immediately when application starts
2. **File Watching**: Uses the `notify` crate to monitor file system events
3. **Debouncing**: Collects file change events and waits 5 seconds before processing
4. **Git Operations**: Runs `git diff --stat` to get an overview and `git diff` for specific files
5. **Terminal UI**: Uses `ratatui` for the split-pane terminal interface
6. **Async Processing**: Uses `tokio` for concurrent file watching and UI updates

## Dependencies

- `clap`: Command line argument parsing
- `crossterm`: Cross-platform terminal handling
- `ratatui`: Terminal UI framework
- `notify`: File system event monitoring
- `tokio`: Async runtime
- `git2`: Git operations (backup, mainly using git CLI)
- `anyhow`: Error handling
- `chrono`: Date/time handling

## Troubleshooting

### "Directory is not a git repository"
Make sure the directory you're trying to watch is a git repository (contains a `.git` folder).

### "Git command failed"
Ensure git is installed and accessible from your PATH. The directory should have some changes to show diffs.

### Application not responding
Try pressing 'r' to manually refresh, or 'q' to quit and restart.

## License

This project is open source and available under the [MIT License](LICENSE). 