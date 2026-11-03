<#
Baseline measurements for the acceptance checklist.
Cold start is read from the log line 'main window ready, startup N ms'.
Memory is the sum of the app process and every descendant (WebView2 children
included), sampled after the window reports ready plus a settle delay.
#>
param(
    [string]$Build = 'release',
    [int]$Runs = 3,
    [string]$OutFile = 'docs/BASELINE.md'
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $repo "src-tauri\target\$Build\mix.exe"
if (-not (Test-Path $bin)) { throw "missing binary: $bin" }

function Get-Descendants([int]$RootId) {
    $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
    $result = @()
    $frontier = @($RootId)
    while ($frontier.Count -gt 0) {
        $next = @()
        foreach ($parent in $frontier) {
            foreach ($child in ($all | Where-Object { $_.ParentProcessId -eq $parent })) {
                if ($result -notcontains $child.ProcessId) {
                    $result += $child.ProcessId
                    $next += $child.ProcessId
                }
            }
        }
        $frontier = $next
    }
    return $result
}

function Get-TreeMemory([int]$RootId) {
    $ids = @($RootId) + (Get-Descendants $RootId)
    $working = 0
    $private = 0
    foreach ($id in $ids) {
        $proc = Get-Process -Id $id -ErrorAction SilentlyContinue
        if ($proc) {
            $working += $proc.WorkingSet64
            $private += $proc.PrivateMemorySize64
        }
    }
    return [pscustomobject]@{
        WorkingMB = [math]::Round($working / 1MB, 0)
        PrivateMB = [math]::Round($private / 1MB, 0)
    }
}

function New-Sandbox([string]$Name, [int]$Tabs) {
    $dir = Join-Path $env:TEMP "mix-baseline-$Name"
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $dir | Out-Null
    Copy-Item $bin (Join-Path $dir 'mix.exe')
    $loader = Join-Path (Split-Path -Parent $bin) 'WebView2Loader.dll'
    if (Test-Path $loader) { Copy-Item $loader (Join-Path $dir 'WebView2Loader.dll') }

    $data = Join-Path $dir 'UserData'
    New-Item -ItemType Directory -Path $data -Force | Out-Null
    $urls = @()
    for ($i = 1; $i -le $Tabs; $i++) { $urls += "https://example.com/?tab=$i" }
    $session = @{ schema_version = 1; tabs = $urls; selected = 0 } | ConvertTo-Json -Compress
    [IO.File]::WriteAllText((Join-Path $data 'session.json'), $session, (New-Object System.Text.UTF8Encoding($false)))
    $settings = @{ schema_version = 1; start_url = 'https://example.com'; restore_session = $true; chrome_height = 68.0; max_live_tabs = 64 } | ConvertTo-Json -Compress
    [IO.File]::WriteAllText((Join-Path $data 'settings.json'), $settings, (New-Object System.Text.UTF8Encoding($false)))
    return $dir
}

function Invoke-Run([string]$Dir, [int]$SettleSeconds) {
    Get-Process mix, msedgewebview2 -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 600
    $log = Join-Path $Dir 'UserData\logs\mix.log'
    Remove-Item $log -Force -ErrorAction SilentlyContinue

    $proc = Start-Process -FilePath (Join-Path $Dir 'mix.exe') -WorkingDirectory $Dir -PassThru
    $startup = $null
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 150
        if (Test-Path $log) {
            $match = Select-String -Path $log -Pattern 'startup ([0-9]+) ms' -Encoding UTF8 | Select-Object -Last 1
            if ($match) { $startup = [int]$match.Matches[0].Groups[1].Value; break }
        }
    }
    Start-Sleep -Seconds $SettleSeconds
    $memory = Get-TreeMemory $proc.Id
    $procs = 1 + (Get-Descendants $proc.Id).Count
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
    Get-Process mix -ErrorAction SilentlyContinue | Stop-Process -Force
    return [pscustomobject]@{ Startup = $startup; PrivateMB = $memory.PrivateMB; WorkingMB = $memory.WorkingMB; Processes = $procs }
}

$rows = @()
foreach ($tabs in @(1, 5, 10)) {
    $dir = New-Sandbox "tabs$tabs" $tabs
    $startups = @()
    $privates = @()
    $workings = @()
    $counts = @()
    for ($i = 1; $i -le $Runs; $i++) {
        $run = Invoke-Run $dir 6
        if ($run.Startup) { $startups += $run.Startup }
        $privates += $run.PrivateMB
        $workings += $run.WorkingMB
        $counts += $run.Processes
    }
    $avgStart = 0
    if ($startups.Count -gt 0) { $avgStart = ($startups | Measure-Object -Average).Average }
    $rows += [pscustomobject]@{
        Tabs         = $tabs
        StartupMs    = [math]::Round($avgStart, 0)
        PrivateMB    = [math]::Round(($privates | Measure-Object -Average).Average, 0)
        WorkingMB    = [math]::Round(($workings | Measure-Object -Average).Average, 0)
        ProcessCount = [math]::Round(($counts | Measure-Object -Average).Average, 1)
    }
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
}

$stamp = Get-Date -Format 'yyyy-MM-dd HH:mm'
$build = (Select-String -Path (Join-Path $repo 'src-tauri\Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"').Matches.Groups[1].Value

$md = @()
$md += '# Baseline'
$md += ''
$md += "Generated by scripts/measure.ps1 on $stamp, build $Build, version $build."
$md += ''
$md += '| Tabs | Cold start (ms) | Private (MB) | Working set (MB) | Processes |'
$md += '| --- | --- | --- | --- | --- |'
foreach ($row in $rows) {
    $md += "| $($row.Tabs) | $($row.StartupMs) | $($row.PrivateMB) | $($row.WorkingMB) | $($row.ProcessCount) |"
}
$md += ''
$md += 'Startup is measured inside the process: from main() to the log line'
$md += 'main window ready, startup N ms. Process creation and antivirus scanning add'
$md += 'to the wall clock a user sees but are not included here.'
$md += ''
$md += 'Memory covers the app process plus every descendant, which is where the WebView2'
$md += 'renderer, GPU and network processes live. Private bytes is the number to compare'
$md += 'against a budget: summing working sets counts shared DLL pages once per process'
$md += 'and inflates the total badly.'
$md += ''
$md += 'Tab switching time is not measured: it needs real input, and this machine'
$md += 'cannot be driven from a script.'

$path = Join-Path $repo $OutFile
[IO.File]::WriteAllText($path, ($md -join [Environment]::NewLine), (New-Object System.Text.UTF8Encoding($false)))
Write-Host "wrote $path"
$rows | Format-Table -AutoSize | Out-String | Write-Host
