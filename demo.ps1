# WatchHound Demo Script
# This script demonstrates the WatchHound application

Write-Host "🐕 WatchHound Demo" -ForegroundColor Green
Write-Host "==================" -ForegroundColor Green
Write-Host ""

Write-Host "1. Starting WatchHound in the background..." -ForegroundColor Yellow
Write-Host "   The application will monitor this directory for file changes" -ForegroundColor Gray
Write-Host ""

Write-Host "2. To test the application:" -ForegroundColor Yellow
Write-Host "   - Run: cargo run -- ." -ForegroundColor Cyan
Write-Host "   - In another terminal, modify test_file.txt" -ForegroundColor Cyan
Write-Host "   - Wait 5 seconds to see the git diff appear" -ForegroundColor Cyan
Write-Host ""

Write-Host "3. Controls:" -ForegroundColor Yellow
Write-Host "   - Press 'q' or 'Esc' to quit" -ForegroundColor Cyan
Write-Host "   - Press 'r' to manually refresh" -ForegroundColor Cyan
Write-Host ""

Write-Host "4. Example file modification:" -ForegroundColor Yellow
Write-Host "   echo 'New line added!' >> test_file.txt" -ForegroundColor Cyan
Write-Host ""

Write-Host "Ready to run? Execute: cargo run -- ." -ForegroundColor Green
Write-Host "Then modify test_file.txt in another terminal!" -ForegroundColor Green 