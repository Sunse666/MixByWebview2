<#
Pack a portable build: binary + loader + readme + empty user folders.
Never pack UserData/, crx/ or javascript/ content: those belong to the user.
#>
param(
    [string]$Target = "release",
    [string]$OutDir = "dist"
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$build = Join-Path $repo "src-tauri\target\$Target"

$exe = Join-Path $build "mix.exe"
if (-not (Test-Path $exe)) {
    throw "missing binary: $exe (run: cargo build --release)"
}

$version = (Select-String -Path (Join-Path $repo "src-tauri\Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"').Matches.Groups[1].Value
if (-not $version) { $version = "0.0.0" }
$stamp = Get-Date -Format "yyyyMMdd"

$stage = Join-Path $repo "$OutDir\Mix-$version-$Target"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Path $stage | Out-Null

Copy-Item $exe (Join-Path $stage "mix.exe")

# WebView2 bootstrap, required next to the binary on Windows.
$loader = Join-Path $build "WebView2Loader.dll"
if (Test-Path $loader) { Copy-Item $loader (Join-Path $stage "WebView2Loader.dll") }

$readme = Join-Path $repo "docs\README-friend.md"
if (Test-Path $readme) { Copy-Item $readme (Join-Path $stage "README.txt") }

# Created by the app on first run; ship them with a placeholder so the zip keeps them.
foreach ($dir in @("crx", "javascript")) {
    $path = Join-Path $stage $dir
    New-Item -ItemType Directory -Path $path | Out-Null
    Set-Content -Path (Join-Path $path "README.txt") -Value "Drop files here, then restart Mix." -Encoding UTF8
}

$leak = Get-ChildItem $stage -Recurse -Force | Where-Object { $_.FullName -match "\\(UserData|profile)\\" }
if ($leak) { throw "user data leaked into the package: $($leak[0].FullName)" }

$zip = "$stage.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal

$exeMb = [math]::Round((Get-Item $exe).Length / 1MB, 2)
$zipMb = [math]::Round((Get-Item $zip).Length / 1MB, 2)
Write-Host "package : $zip"
Write-Host "contents: mix.exe ($exeMb MB) + loader + README.txt + empty crx/ and javascript/"
Write-Host "zip size: $zipMb MB"