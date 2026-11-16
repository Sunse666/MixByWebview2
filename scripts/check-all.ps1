# Runs every check that needs no window: cargo test, clippy, rustfmt, internal pages,
# i18n and the command lists.
$repo = Split-Path -Parent $PSScriptRoot
$failed = @()

# rustup can report the clippy and rustfmt components as installed for a toolchain while
# the binaries are not on disk, and cargo then refuses them with "not applicable to the
# toolchain". Take the first installed toolchain that actually ships the binary, so the
# gate runs here instead of being skipped.
function Find-ToolchainWith($binary) {
  foreach ($name in (& rustup toolchain list 2>$null | ForEach-Object { ($_ -split '\s+')[0] })) {
    $root = & rustup run $name rustc --print sysroot 2>$null
    if ($root -and (Test-Path (Join-Path $root "bin\$binary"))) { return $name }
  }
  return $null
}

Push-Location (Join-Path $repo "src-tauri")
$out = & cargo test --offline 2>&1 | Out-String
$code = $LASTEXITCODE
($out -split "`n" | Select-String -Pattern "test result|^error") | ForEach-Object { $_.Line }
if ($code -ne 0) { $failed += "cargo test" }

$clippy = Find-ToolchainWith "cargo-clippy.exe"
if ($clippy) {
  $out = & cargo "+$clippy" clippy --offline --all-targets -- -D warnings 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0) {
    ($out -split "`n" | Select-String -Pattern "^error|^warning" | Select-Object -First 20) |
      ForEach-Object { $_.Line }
    $failed += "cargo clippy"
  } else {
    Write-Host "ok   clippy ($clippy)"
  }
} else {
  Write-Host "skipped clippy: no installed toolchain ships cargo-clippy" -ForegroundColor Yellow
}

$fmt = Find-ToolchainWith "cargo-fmt.exe"
if ($fmt) {
  $out = & cargo "+$fmt" fmt --check 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0) {
    ($out -split "`n" | Select-Object -First 20) | ForEach-Object { $_.Line }
    $failed += "cargo fmt"
  } else {
    Write-Host "ok   rustfmt"
  }
} else {
  Write-Host "skipped rustfmt: no installed toolchain ships cargo-fmt" -ForegroundColor Yellow
}
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
