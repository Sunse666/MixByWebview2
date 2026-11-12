# Runs every check that needs no window: cargo test, internal pages, i18n, command lists.
$repo = Split-Path -Parent $PSScriptRoot
$failed = @()

Push-Location (Join-Path $repo "src-tauri")
$out = & cargo test --offline 2>&1 | Out-String
$code = $LASTEXITCODE
($out -split "`n" | Select-String -Pattern "test result|^error") | ForEach-Object { $_.Line }
if ($code -ne 0) { $failed += "cargo test" }
Pop-Location

Push-Location $repo
foreach ($script in @("check-pages.js", "check-i18n.js")) {
  & node "scripts/$script"
  if ($LASTEXITCODE -ne 0) { $failed += $script }
}
& node "scripts/gen-commands.js" --check
if ($LASTEXITCODE -ne 0) { $failed += "gen-commands" }
Pop-Location

if ($failed.Count -gt 0) {
  Write-Host ("FAILED: " + ($failed -join ", ")) -ForegroundColor Red
  exit 1
}
Write-Host "all checks passed" -ForegroundColor Green
