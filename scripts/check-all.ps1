# Everything that can be checked without opening a window.
#
# Run this before packaging: a page that fails to build, a string that skipped the
# dictionary, or a chrome that no longer boots are all silent failures otherwise.
#
# Exit codes are checked explicitly, so a native command writing to stderr is not treated
# as a failure by PowerShell itself.
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
Pop-Location

if ($failed.Count -gt 0) {
  Write-Host ("FAILED: " + ($failed -join ", ")) -ForegroundColor Red
  exit 1
}
Write-Host "all checks passed" -ForegroundColor Green
