# Test script to create multiple files with changes for navigation testing

Write-Host "🐕 Setting up multiple test files for WatchHound navigation..." -ForegroundColor Green

# Create first test file
Write-Host "Creating file1.txt..." -ForegroundColor Yellow
"Original content in file 1" | Out-File -FilePath "file1.txt" -Encoding utf8

# Create second test file
Write-Host "Creating file2.txt..." -ForegroundColor Yellow
"Original content in file 2" | Out-File -FilePath "file2.txt" -Encoding utf8

# Create third test file
Write-Host "Creating file3.txt..." -ForegroundColor Yellow
"Original content in file 3" | Out-File -FilePath "file3.txt" -Encoding utf8

# Add and commit these files
Write-Host "Adding files to git..." -ForegroundColor Yellow
git add file1.txt file2.txt file3.txt
git commit -m "Add test files for navigation"

# Now modify all files to create diffs
Write-Host "Modifying all files to create diffs..." -ForegroundColor Yellow
"Original content in file 1" | Out-File -FilePath "file1.txt" -Encoding utf8
"MODIFIED line added to file 1!" | Out-File -FilePath "file1.txt" -Append -Encoding utf8
"Another change in file 1" | Out-File -FilePath "file1.txt" -Append -Encoding utf8

"Original content in file 2" | Out-File -FilePath "file2.txt" -Encoding utf8
"MODIFIED line added to file 2!" | Out-File -FilePath "file2.txt" -Append -Encoding utf8
"Different change in file 2" | Out-File -FilePath "file2.txt" -Append -Encoding utf8

"Original content in file 3" | Out-File -FilePath "file3.txt" -Encoding utf8
"MODIFIED line added to file 3!" | Out-File -FilePath "file3.txt" -Append -Encoding utf8
"Yet another change in file 3" | Out-File -FilePath "file3.txt" -Append -Encoding utf8

Write-Host ""
Write-Host "✅ Test files created with changes!" -ForegroundColor Green
Write-Host ""
Write-Host "Now run: cargo run -- ." -ForegroundColor Cyan
Write-Host ""
Write-Host "Navigation controls:" -ForegroundColor Yellow
Write-Host "  ← → : Navigate between files" -ForegroundColor White
Write-Host "  Space: Scroll down the current diff" -ForegroundColor White
Write-Host "  r: Refresh manually" -ForegroundColor White
Write-Host "  q: Quit" -ForegroundColor White
Write-Host ""
Write-Host "You should see 3 files to navigate through!" -ForegroundColor Green 