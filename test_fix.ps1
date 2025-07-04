#!/usr/bin/env pwsh
# Test script to verify the file change fix

Write-Host "Testing file change fix..." -ForegroundColor Green
Write-Host "This script will modify different files to test that the correct file is displayed" -ForegroundColor Yellow
Write-Host ""

# Make sure we have some changes first
Write-Host "1. Modifying file2.txt..." -ForegroundColor Cyan
Add-Content -Path "file2.txt" -Value "Test change for file2 - $(Get-Date)"

Write-Host "2. Modifying file3.txt..." -ForegroundColor Cyan  
Add-Content -Path "file3.txt" -Value "Test change for file3 - $(Get-Date)"

Write-Host "3. Modifying file1.txt..." -ForegroundColor Cyan
Add-Content -Path "file1.txt" -Value "Test change for file1 - $(Get-Date)"

Write-Host ""
Write-Host "All files modified. Now you can:" -ForegroundColor Green
Write-Host "1. Run 'cargo run' to start WatchHound" -ForegroundColor White
Write-Host "2. In another terminal, run this script again to modify files" -ForegroundColor White
Write-Host "3. Watch that the diff shows the correct file that was just modified" -ForegroundColor White
Write-Host ""
Write-Host "Before the fix: Always showed file1.txt (first file)" -ForegroundColor Red
Write-Host "After the fix: Should show the actual file that changed" -ForegroundColor Green 