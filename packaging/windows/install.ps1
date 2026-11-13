<#
.SYNOPSIS
  Install Wisp for the current user. No administrator rights are needed.

.DESCRIPTION
  Copies Wisp to %LOCALAPPDATA%\Programs\Wisp, adds Start menu and desktop
  shortcuts, and registers Wisp under Settings > Apps so it can be uninstalled
  like any other app. Running it again updates Wisp in place; your settings
  and WhatsApp sessions are kept.

.EXAMPLE
  .\install.ps1                # install or update, then start Wisp
  .\install.ps1 -NoDesktop     # without a desktop shortcut
  .\install.ps1 -Uninstall     # remove Wisp, keep settings and sessions
  .\install.ps1 -Uninstall -Purge   # also delete settings and sessions
#>
param(
    [switch] $Uninstall,
    [switch] $Purge,
    [switch] $NoDesktop,
    [switch] $NoLaunch,
    # Wisp.exe to install. Default: next to this script (release zip), or the
    # release build of this repository.
    [string] $Source
)

$ErrorActionPreference = 'Stop'

$AppDir = Join-Path $env:LOCALAPPDATA 'Programs\Wisp'
$Exe = Join-Path $AppDir 'Wisp.exe'
$StartMenuLink = Join-Path ([Environment]::GetFolderPath('Programs')) 'Wisp.lnk'
$DesktopLink = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Wisp.lnk'
$UninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Wisp'

function Stop-Wisp {
    # Every copy shares the same sessions, so close them all before replacing files.
    $running = @(Get-Process -Name wisp -ErrorAction SilentlyContinue)
    if ($running.Count -eq 0) { return }
    Write-Host 'Closing Wisp...'
    $running | ForEach-Object { [void]$_.CloseMainWindow() }
    Start-Sleep -Seconds 2
    Get-Process -Name wisp -ErrorAction SilentlyContinue | Stop-Process -Force
    # Give WebView2 a moment to release its files.
    Start-Sleep -Seconds 2
}

function New-Shortcut([string] $Path) {
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($Path)
    $link.TargetPath = $Exe
    $link.WorkingDirectory = $AppDir
    $link.IconLocation = "$Exe,0"
    $link.Description = 'A lightweight desktop app for WhatsApp Web'
    $link.Save()
}

if ($Uninstall) {
    Stop-Wisp
    Remove-Item $StartMenuLink, $DesktopLink -Force -ErrorAction SilentlyContinue
    Remove-Item $UninstallKey -Recurse -Force -ErrorAction SilentlyContinue
    # Starting at sign-in (turned on in Wisp's Settings).
    Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name Wisp -ErrorAction SilentlyContinue
    # Wisp's notification identity, and the notification settings Windows keeps for it.
    foreach ($key in 'HKCU:\Software\Classes\AppUserModelId\io.github.felicerossetti.wisp',
                     'HKCU:\Software\Microsoft\Windows\CurrentVersion\Notifications\Settings\io.github.felicerossetti.wisp') {
        Remove-Item $key -Recurse -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path $AppDir) { Remove-Item $AppDir -Recurse -Force }
    if ($Purge) {
        foreach ($dir in (Join-Path $env:APPDATA 'Wisp'), (Join-Path $env:LOCALAPPDATA 'Wisp')) {
            if (Test-Path $dir) { Remove-Item $dir -Recurse -Force }
        }
        Write-Host 'Wisp, its settings and its WhatsApp sessions were removed.'
    } else {
        Write-Host 'Wisp was removed. Your settings and sessions are kept; reinstalling picks them up again.'
    }
    return
}

# ---- Find the program to install ---------------------------------------------
if (-not $Source) {
    $candidates = @(
        (Join-Path $PSScriptRoot 'Wisp.exe'),
        (Join-Path $PSScriptRoot '..\..\target\release\wisp.exe')
    )
    $Source = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Source -or -not (Test-Path $Source)) {
    throw "Couldn't find Wisp.exe. Put this script next to Wisp.exe, build with 'cargo build --release', or pass -Source."
}
$Source = (Resolve-Path $Source).Path

# ---- Install -------------------------------------------------------------------
Stop-Wisp
New-Item -ItemType Directory -Force -Path $AppDir | Out-Null
Copy-Item $Source $Exe -Force
# Keep a copy of this script so Settings > Apps can uninstall Wisp.
if ($PSCommandPath -ne (Join-Path $AppDir 'install.ps1')) {
    Copy-Item $PSCommandPath (Join-Path $AppDir 'install.ps1') -Force
}

New-Shortcut $StartMenuLink
if ($NoDesktop) {
    Remove-Item $DesktopLink -Force -ErrorAction SilentlyContinue
} else {
    New-Shortcut $DesktopLink
}

# Settings > Apps entry, so Wisp uninstalls like any other app.
$version = (Get-Item $Exe).VersionInfo.ProductVersion
$sizeKb = [int]((Get-ChildItem $AppDir -Recurse -File | Measure-Object Length -Sum).Sum / 1KB)
$uninstallCommand = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$AppDir\install.ps1`" -Uninstall"
New-Item -Path $UninstallKey -Force | Out-Null
$values = @{
    DisplayName          = 'Wisp'
    DisplayVersion       = "$version"
    DisplayIcon          = "$Exe,0"
    Publisher            = 'Wisp'
    InstallLocation      = $AppDir
    UninstallString      = $uninstallCommand
    QuietUninstallString = $uninstallCommand
    URLInfoAbout         = 'https://github.com/FeliceRossetti/Wisp'
}
foreach ($k in $values.Keys) { Set-ItemProperty -Path $UninstallKey -Name $k -Value $values[$k] }
Set-ItemProperty -Path $UninstallKey -Name NoModify -Value 1 -Type DWord
Set-ItemProperty -Path $UninstallKey -Name NoRepair -Value 1 -Type DWord
Set-ItemProperty -Path $UninstallKey -Name EstimatedSize -Value $sizeKb -Type DWord

Write-Host "Wisp $version is installed in $AppDir"
Write-Host ('Find it in the Start menu' + $(if ($NoDesktop) { '.' } else { ' and on your desktop.' }))

if (-not $NoLaunch) { Start-Process $Exe }
