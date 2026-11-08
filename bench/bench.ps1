<#
.SYNOPSIS
  Compare Wisp with the Microsoft Store WhatsApp on Windows.

.DESCRIPTION
  Both apps host WhatsApp Web in WebView2, so every measurement covers the
  whole process tree (the host process plus all msedgewebview2.exe children).

  -Mode Snapshot  Measure the apps as they are running now (no restarts).
  -Mode Background Restart both apps, leave them visible but unfocused, measure;
                  then minimize both, wait, and measure again. Closes and reopens both apps.
  -Mode ColdStart Close each app, start it, and time how long it takes to
                  show a window and to settle, then measure it. Repeats -Runs times.
                  This closes and reopens both apps.

  Results are printed and saved as JSON next to this script.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File bench\bench.ps1 -Mode Snapshot
  powershell -ExecutionPolicy Bypass -File bench\bench.ps1 -Mode ColdStart -Runs 3
#>
param(
    [ValidateSet('Snapshot', 'ColdStart', 'Background')] [string] $Mode = 'Snapshot',
    [int] $Runs = 3,
    [int] $CpuSeconds = 30,     # how long to sample idle CPU
    [int] $UnfocusedMinutes = 5, # Background mode: time left visible but unfocused
    [int] $MinimizedMinutes = 3, # Background mode: time left minimized
    [int] $WarmSeconds = 45,    # watch a cold start this long, then measure memory and idle CPU
    [string[]] $Only = @(),     # limit to these apps, e.g. -Only "Wisp,WhatsApp (Store)"
    [string] $WispExe = (Join-Path $PSScriptRoot '..\target\release\wisp.exe')
)

$ErrorActionPreference = 'Stop'
# -Only arrives as one string when run with -File; accept a comma-separated list.
$Only = @($Only | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim([char[]]" '`"") } | Where-Object { $_ })
$Cores = [Environment]::ProcessorCount

$WispConfig = Join-Path $env:APPDATA 'Wisp\config\config.toml'

function Set-WispTheme([bool] $on) {
    if (-not (Test-Path $WispConfig)) { return }
    $v = if ($on) { 'true' } else { 'false' }
    $text = [IO.File]::ReadAllText($WispConfig) -replace '(?m)^theme_enabled = .*$', "theme_enabled = $v"
    # Windows PowerShell's "utf8" adds a byte-order mark, which TOML parsers reject.
    [IO.File]::WriteAllText($WispConfig, $text, [Text.UTF8Encoding]::new($false))
}

$Apps = @(
    [pscustomobject]@{
        Name   = 'Wisp'
        Host   = 'wisp'
        Launch = { Start-Process -FilePath (Resolve-Path $WispExe) }
        Disk   = { (Get-Item (Resolve-Path $WispExe)).Length }
    },
    [pscustomobject]@{
        Name   = 'Wisp (no theme)'
        Host   = 'wisp'
        Launch = { Set-WispTheme $false; Start-Process -FilePath (Resolve-Path $WispExe) }
        Disk   = { (Get-Item (Resolve-Path $WispExe)).Length }
        ColdStartOnly = $true
    },
    [pscustomobject]@{
        Name   = 'WhatsApp (Store)'
        Host   = 'WhatsApp.Root'
        Launch = {
            $pkg = Get-AppxPackage 5319275A.WhatsAppDesktop
            Start-Process "shell:AppsFolder\$($pkg.PackageFamilyName)!App"
        }
        Disk   = {
            $pkg = Get-AppxPackage 5319275A.WhatsAppDesktop
            (Get-ChildItem $pkg.InstallLocation -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
        }
    }
)

function Get-TreeIds([string] $hostName) {
    $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name
    $roots = @($all | Where-Object { $_.Name -eq "$hostName.exe" } | ForEach-Object ProcessId)
    $ids = [System.Collections.Generic.List[int]]::new()
    $queue = [System.Collections.Generic.Queue[int]]::new()
    foreach ($r in $roots) { $queue.Enqueue([int]$r) }
    while ($queue.Count) {
        $id = $queue.Dequeue()
        if ($ids.Contains($id)) { continue }
        $ids.Add($id)
        foreach ($c in $all) { if ($c.ParentProcessId -eq $id) { $queue.Enqueue([int]$c.ProcessId) } }
    }
    , $ids
}

function Get-TreeProcs([string] $hostName) {
    $ids = Get-TreeIds $hostName
    @($ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
}

# CPU time is tracked per process: $seen maps PID -> the highest CPU time seen for it.
# Summing a fresh reading of the whole tree is wrong because WebView2 starts and
# exits helper processes, and an exited process would take its CPU time with it.
function Update-Cpu([string] $hostName, [hashtable] $seen) {
    foreach ($p in Get-TreeProcs $hostName) {
        try { $t = $p.TotalProcessorTime.TotalSeconds } catch { continue }
        if (-not $seen.ContainsKey($p.Id) -or $t -gt $seen[$p.Id]) { $seen[$p.Id] = $t }
    }
    ($seen.Values | Measure-Object -Sum).Sum
}

# Average CPU of each app's tree over the same $seconds, as % of one core.
function Measure-IdleCpu([string[]] $hostNames, [int] $seconds) {
    $seen = @{}; $start = @{}
    foreach ($h in $hostNames) { $seen[$h] = @{}; $start[$h] = Update-Cpu $h $seen[$h] }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    while ($sw.Elapsed.TotalSeconds -lt $seconds) {
        Start-Sleep -Seconds 1
        foreach ($h in $hostNames) { $null = Update-Cpu $h $seen[$h] }
    }
    $res = @{}
    foreach ($h in $hostNames) { $res[$h] = [math]::Round(100 * ((Update-Cpu $h $seen[$h]) - $start[$h]) / $sw.Elapsed.TotalSeconds, 2) }
    $res
}

function Measure-Memory([string] $hostName) {
    $procs = Get-TreeProcs $hostName
    $hostProc = $procs | Where-Object { $_.ProcessName -eq $hostName } | Select-Object -First 1
    [pscustomobject]@{
        Processes   = $procs.Count
        PrivateMB   = [math]::Round(($procs | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB, 1)
        WorkingSetMB = [math]::Round(($procs | Measure-Object WorkingSet64 -Sum).Sum / 1MB, 1)
        HostPrivateMB = if ($hostProc) { [math]::Round($hostProc.PrivateMemorySize64 / 1MB, 1) } else { $null }
        Threads     = ($procs | ForEach-Object { $_.Threads.Count } | Measure-Object -Sum).Sum
        Handles     = ($procs | Measure-Object HandleCount -Sum).Sum
    }
}

function Stop-App($app) {
    $ids = Get-TreeIds $app.Host
    foreach ($id in $ids) { Stop-Process -Id $id -Force -ErrorAction SilentlyContinue }
    $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Process -Name $app.Host -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 200 }
    Start-Sleep -Seconds 3   # let WebView2 release its profile
}

function Measure-ColdStart($app) {
    Stop-App $app
    $seen = @{}   # every process here started after launch, so totals = CPU since launch
    $sw = [Diagnostics.Stopwatch]::StartNew()
    & $app.Launch
    $window = $null; $settled = $null; $startupCpu = $null
    $quiet = 0; $lastCpu = 0.0; $lastT = 0.0
    while ($sw.Elapsed.TotalSeconds -lt $WarmSeconds) {
        Start-Sleep -Milliseconds 250
        if (-not $window) {
            $h = Get-Process -Name $app.Host -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 }
            if ($h) { $window = $sw.Elapsed.TotalSeconds }
        }
        $t = $sw.Elapsed.TotalSeconds
        if ($t - $lastT -ge 1.0) {
            $cpu = Update-Cpu $app.Host $seen
            # Settled = window shown and tree CPU under 5% of one core for 3 s in a row.
            if (-not $settled) {
                $pct = 100 * ($cpu - $lastCpu) / ($t - $lastT)
                if ($lastT -gt 0 -and $window -and $pct -lt 5) { $quiet++ } else { $quiet = 0 }
                if ($quiet -ge 3) { $settled = $t - 3 }
            }
            $lastCpu = $cpu; $lastT = $t
        }
    }
    $startupCpu = Update-Cpu $app.Host $seen
    $mem = Measure-Memory $app.Host
    $idleCpu = (Measure-IdleCpu @($app.Host) $CpuSeconds)[$app.Host]
    [pscustomobject]@{
        App              = $app.Name
        WindowSec        = if ($window) { [math]::Round($window, 2) } else { $null }
        SettledSec       = if ($settled) { [math]::Round($settled, 2) } else { $null }
        CpuSecFirst45s   = [math]::Round($startupCpu, 2)
        PrivateMB        = $mem.PrivateMB
        WorkingSetMB     = $mem.WorkingSetMB
        HostPrivateMB    = $mem.HostPrivateMB
        Processes        = $mem.Processes
        IdleCpuPctOfCore = $idleCpu
    }
}


Add-Type -Namespace Bench -Name Win -MemberDefinition @"
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
"@

function Get-MainWindow([string] $hostName) {
    Get-Process -Name $hostName -ErrorAction SilentlyContinue |
        Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1 -ExpandProperty MainWindowHandle
}

function Measure-Row($app, [hashtable] $cpu, [string] $state) {
    $mem = Measure-Memory $app.Host
    [pscustomobject]@{
        State            = $state
        App              = $app.Name
        PrivateMB        = $mem.PrivateMB
        WorkingSetMB     = $mem.WorkingSetMB
        HostPrivateMB    = $mem.HostPrivateMB
        Processes        = $mem.Processes
        IdleCpuPctOfCore = $cpu[$app.Host]
    }
}

$cpuName = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
Write-Host "Wisp vs Store WhatsApp  |  $Mode  |  $cpuName, $Cores threads`n"

$results = [ordered]@{ Mode = $Mode; Date = (Get-Date).ToString('s'); Cpu = $cpuName; Threads = $Cores }

if ($Mode -eq 'Snapshot') {
    $rows = foreach ($app in ($Apps | Where-Object { -not $_.ColdStartOnly -and (-not $Only -or $Only -contains $_.Name) })) {
        if (-not (Get-Process -Name $app.Host -ErrorAction SilentlyContinue)) {
            Write-Warning "$($app.Name) is not running; skipping"; continue
        }
        $mem = Measure-Memory $app.Host
        [pscustomobject]@{
            App           = $app.Name
            PrivateMB     = $mem.PrivateMB
            WorkingSetMB  = $mem.WorkingSetMB
            HostPrivateMB = $mem.HostPrivateMB
            Processes     = $mem.Processes
            Threads       = $mem.Threads
            Handles       = $mem.Handles
            DiskMB        = [math]::Round((& $app.Disk) / 1MB, 1)
        }
    }
    # Sample CPU for all apps at the same time so background activity hits them equally.
    $hostOf = @{}; foreach ($a in $Apps) { $hostOf[$a.Name] = $a.Host }
    $cpu = Measure-IdleCpu @($rows | ForEach-Object { $hostOf[$_.App] }) $CpuSeconds
    foreach ($r in $rows) { $r | Add-Member IdleCpuPctOfCore $cpu[$hostOf[$r.App]] }
    $rows | Format-Table -AutoSize | Out-String | Write-Host
    $results.Snapshot = $rows
}
elseif ($Mode -eq 'Background') {
    $pair = @($Apps | Where-Object { -not $_.ColdStartOnly })
    foreach ($app in $pair) { Stop-App $app }
    foreach ($app in $pair) { & $app.Launch; Start-Sleep -Seconds 5 }
    Start-Sleep -Seconds 30
    # Minimize, then restore without activating: both visible, neither focused.
    $handles = @{}; foreach ($app in $pair) { $handles[$app.Host] = Get-MainWindow $app.Host }
    foreach ($h in $handles.Values) { [void][Bench.Win]::ShowWindow($h, 6) }   # SW_MINIMIZE
    Start-Sleep -Seconds 2
    foreach ($h in $handles.Values) { [void][Bench.Win]::ShowWindow($h, 4) }   # SW_SHOWNOACTIVATE
    $hosts = @($pair | ForEach-Object Host)

    Write-Host "Visible but unfocused for $UnfocusedMinutes min..."
    Start-Sleep -Seconds (60 * $UnfocusedMinutes)
    $cpu = Measure-IdleCpu $hosts $CpuSeconds
    $rows = @(foreach ($app in $pair) { Measure-Row $app $cpu 'unfocused' })

    Write-Host "Minimized for $MinimizedMinutes min..."
    foreach ($h in $handles.Values) { [void][Bench.Win]::ShowWindow($h, 6) }
    Start-Sleep -Seconds (60 * $MinimizedMinutes)
    $cpu = Measure-IdleCpu $hosts $CpuSeconds
    $rows += @(foreach ($app in $pair) { Measure-Row $app $cpu 'minimized' })

    foreach ($h in $handles.Values) { [void][Bench.Win]::ShowWindow($h, 4) }   # leave them on screen
    $rows | Format-Table -AutoSize | Out-String | Write-Host
    $results.Background = $rows
}
else {
    $all = foreach ($i in 1..$Runs) {
        foreach ($app in ($Apps | Where-Object { -not $Only -or $Only -contains $_.Name })) {
            Write-Host "Run $i/$Runs - $($app.Name)..."
            try { Measure-ColdStart $app } finally { Set-WispTheme $true }
        }
    }
    Set-WispTheme $true
    $all | Format-Table -AutoSize | Out-String | Write-Host
    $summary = $all | Group-Object App | ForEach-Object {
        $g = $_.Group
        [pscustomobject]@{
            App           = $_.Name
            WindowSec     = [math]::Round(($g | Measure-Object WindowSec -Average).Average, 2)
            SettledSec    = [math]::Round(($g | Measure-Object SettledSec -Average).Average, 2)
            CpuSecFirst45s = [math]::Round(($g | Measure-Object CpuSecFirst45s -Average).Average, 2)
            PrivateMB     = [math]::Round(($g | Measure-Object PrivateMB -Average).Average, 1)
            HostPrivateMB = [math]::Round(($g | Measure-Object HostPrivateMB -Average).Average, 1)
            IdleCpuPctOfCore = [math]::Round(($g | Measure-Object IdleCpuPctOfCore -Average).Average, 2)
        }
    }
    Write-Host "Averages over $Runs runs:"
    $summary | Format-Table -AutoSize | Out-String | Write-Host
    $results.Runs = $all; $results.Summary = $summary
}

$out = Join-Path $PSScriptRoot ("results-{0}-{1}.json" -f $Mode.ToLower(), (Get-Date -Format 'yyyyMMdd-HHmmss'))
$results | ConvertTo-Json -Depth 5 | Set-Content -Encoding utf8 $out
Write-Host "Saved $out"
