# Example Usage of WatchHound

## Basic Usage

1. **Initialize a git repository (if you don't have one)**:
   ```bash
   git init
   echo "# My Project" > README.md
   git add README.md
   git commit -m "Initial commit"
   ```

2. **Run WatchHound**:
   ```bash
   # Watch current directory (must be a git repository)
   cargo run -- .
   
   # Or watch a specific directory
   cargo run -- /path/to/your/git/repo
   ```

3. **Make some changes to test**:
   - Edit a file in the watched directory
   - Add a new file
   - Delete a file
   - After 5 seconds, WatchHound will automatically update the display

## Example Session

### Step 1: Start WatchHound
```bash
cargo run -- .
```

### Step 2: The interface will show
```
┌─ Git Status ──────────────────────┐│┌─ Git Diff ────────────────────────┐
│Waiting for file changes...        ││No changes to show                 │
│                                   ││                                   │
│                                   ││                                   │
│                                   ││                                   │
│                                   ││                                   │
│                                   ││                                   │
└───────────────────────────────────┘│└───────────────────────────────────┘
```

### Step 3: Edit a file
```bash
# In another terminal
echo "New content" >> README.md
```

### Step 4: After 5 seconds, WatchHound updates to show:
```
┌─ Git Status ──────────────────────┐│┌─ Git Diff - README.md ────────────┐
│ README.md | 1 +                   ││@@ -1 +1,2 @@                       │
│ 1 file changed, 1 insertion(+)    ││ # My Project                      │
│                                   ││+New content                       │
│                                   ││                                   │
│                                   ││                                   │
│                                   ││                                   │
└───────────────────────────────────┘│└───────────────────────────────────┘
Last updated: 14:32:15
```

## Controls

- **q** or **Esc**: Quit the application
- **r**: Manually refresh the display (force update without waiting for file changes)

## Common Use Cases

1. **Development Workflow**: Keep WatchHound running while coding to see real-time git diff information
2. **Code Review**: Monitor what files are being changed and how
3. **Debugging**: See exactly what changes are happening in your repository
4. **Learning Git**: Understand how different operations affect your git repository

## Tips

- The application works best with repositories that have some staged or unstaged changes
- If you don't see any changes, try running `git status` manually to see if there are any changes to show
- The 5-second delay prevents excessive git operations during rapid file changes
- Use manual refresh (r) if you want to see changes immediately 