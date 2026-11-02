<#
Pack a portable build: binary + README + empty user dirs.
Never pack UserData/, crx/ or javascript/ (they belong to the user).
#>
param(
    [string]$Target = "release",
    [string]$OutDir = "dist"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $root "target\$Target\Mix.exe"
if (-not (Test-Path $exe)) {
    throw "missing binary: $exe (run: cargo tauri build)"
}

$version = (Select-String -Path (Join-Path $root "src-tauri\Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"').Matches.Groups[1].Value
if (-not $version) { $version = "0.0.0" }

$stage = Join-Path $root "$OutDir\Mix-portable-$version"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Path $stage | Out-Null
New-Item -ItemType Directory -Path (Join-Path $stage "crx") | Out-Null
New-Item -ItemType Directory -Path (Join-Path $stage "javascript") | Out-Null

Copy-Item $exe (Join-Path $stage "Mix.exe")
$readme = Join-Path $root "docs\README-friend.md"
if (Test-Path $readme) { Copy-Item $readme (Join-Path $stage "README.txt") }

$zip = "$stage.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip

Write-Host "packed: $zip"
Write-Host "check that UserData/, crx/ and javascript/ content is NOT inside."