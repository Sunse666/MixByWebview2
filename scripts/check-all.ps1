# Runs every check that needs no window: cargo test, clippy, rustfmt, internal pages,
# i18n and the command lists.
$repo = Split-Path -Parent $PSScriptRoot
$failed = @()

# A toolchain named in MIX_TOOLCHAIN wins and is used for everything. CI sets it, because
# `rustup default` is not enough there: the runner keeps another toolchain active through
# the environment, so `(default)` never moved and the gate linted with the runner's newer
# stable - and failed on lints this tree was never cleaned against.
$pinned = $env:MIX_TOOLCHAIN

# rustup can report the clippy and rustfmt components as installed for a toolchain while
# the binaries are not on disk, and cargo then refuses them with "not applicable to the
# toolchain". So the toolchain is chosen by looking at the files - the pinned one first,
# then the default, because the checks should run with the same compiler `cargo test` used.
function Find-ToolchainWith($binary) {
  $listed = & rustup toolchain list 2>$null
  $names = @($listed | ForEach-Object { ($_ -split '\s+')[0] })
  $default = @($listed | Where-Object { $_ -match '\(default\)' } | ForEach-Object { ($_ -split '\s+')[0] })
  $ordered = @()
  if ($pinned) { $ordered += $names | Where-Object { $_ -like "$pinned*" } }
  $ordered += $default + @($names | Where-Object { $default -notcontains $_ })
  foreach ($name in $ordered) {
    $root = & rustup run $name rustc --print sysroot 2>$null
    if ($root -and (Test-Path (Join-Path $root "bin\$binary"))) { return $name }
  }
  return $null
}

# `cargo +toolchain` when one is pinned, plain `cargo` otherwise.
function Invoke-Cargo($arguments) {
  if ($pinned) { & cargo "+$pinned" @arguments } else { & cargo @arguments }
}

Push-Location (Join-Path $repo "src-tauri")
if ($pinned) { Write-Host "toolchain: $pinned (from MIX_TOOLCHAIN)" }

$out = Invoke-Cargo @("test", "--offline") 2>&1 | Out-String
$code = $LASTEXITCODE
($out -split "`n" | Select-String -Pattern "test result|^error") | ForEach-Object { $_.Line }
if ($code -ne 0) { $failed += "cargo test" }

$clippy = Find-ToolchainWith "cargo-clippy.exe"
if ($clippy) {
  $out = & cargo "+$clippy" clippy --offline --all-targets -- -D warnings 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0) {
    ($out -split "`n" | Select-String -Pattern "^error|^warning" | Select-Object -First 20) |
      ForEach-Object { $_.Line }
    # Naming the toolchain here matters: a failure looks identical whether the code has a
    # problem or the gate picked a different clippy than the one the tree was cleaned with.
    $failed += "cargo clippy ($clippy)"
  } else {
    Write-Host "ok   clippy ($clippy)"
  }
} else {
  Write-Host "skipped clippy: no installed toolchain ships cargo-clippy" -ForegroundColor Yellow
  if ($pinned) { $failed += "clippy under $pinned" }
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
  if ($pinned) { $failed += "rustfmt under $pinned" }
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
