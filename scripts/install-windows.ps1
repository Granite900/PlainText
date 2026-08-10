# One-step Windows setup for PlainText.
#
# Run this from the unzipped release folder:
#
#     powershell -ExecutionPolicy Bypass -File scripts\install-windows.ps1
#
# It does the fiddly parts for you:
#   1. unblocks the binary (clears the "downloaded from the internet" flag that
#      makes SmartScreen nag),
#   2. copies plaintext.exe into a per-user programs folder,
#   3. adds that folder to your PATH, so you can just type `plaintext` anywhere.
#
# Options (most people need neither):
#   -InstallDir <path>   install somewhere other than %LOCALAPPDATA%\Programs\PlainText
#   -SkipPath            copy the binary but don't touch PATH

param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\PlainText'),
    [switch]$SkipPath
)

$ErrorActionPreference = 'Stop'

# Smart App Control (Windows 11) blocks unsigned downloaded executables outright
# -- stricter than SmartScreen, and Unblock-File does NOT clear it. Detect it so
# we can explain the block instead of dying on a confusing error.
function Get-SmartAppControlState {
    try {
        $v = Get-ItemPropertyValue -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy' `
            -Name 'VerifiedAndReputablePolicyState' -ErrorAction Stop
        switch ($v) { 1 { 'On' } 2 { 'Evaluation' } default { 'Off' } }
    } catch { 'Off' }
}

function Show-SmartAppControlHelp {
    Write-Host ''
    Write-Host 'Smart App Control is blocking plaintext.exe.' -ForegroundColor Yellow
    Write-Host 'Windows 11''s Smart App Control refuses unsigned downloaded programs, and'
    Write-Host 'this release is not code-signed. PlainText is safe, but Windows cannot'
    Write-Host 'confirm that on its own. Pick whichever fits you:'
    Write-Host '  1. Build from source so the binary is local, not "downloaded":'
    Write-Host '        cargo build --release   (needs Rust + LLVM; see GETTING-STARTED.md)'
    Write-Host '  2. Run PlainText on a PC without Smart App Control.'
    Write-Host '  3. Turn Smart App Control off: Windows Security > App & browser control'
    Write-Host '     > Smart App Control settings > Off.'
    Write-Host '     WARNING: on a fresh Windows install this switch is ONE-WAY -- it can'
    Write-Host '     only be re-enabled by resetting Windows. Only do this if you accept that.'
    Write-Host ''
}

# Find plaintext.exe: one level up from this script inside the release zip, or
# in the current folder if you moved things around.
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$bin = Join-Path $scriptDir '..\plaintext.exe'
if (-not (Test-Path $bin)) {
    if (Test-Path '.\plaintext.exe') {
        $bin = '.\plaintext.exe'
    } else {
        Write-Error "Couldn't find plaintext.exe. Run this from the unzipped release folder (the one containing plaintext.exe)."
        exit 1
    }
}
$bin = (Resolve-Path $bin).Path
Write-Host "Found PlainText at: $bin"

$sac = Get-SmartAppControlState
if ($sac -eq 'On') {
    Write-Host ''
    Write-Host 'Heads up: Smart App Control is ON. It may block this unsigned binary.' -ForegroundColor Yellow
    Write-Host 'Continuing to install; if the check at the end fails, read the guidance shown.'
}

# 1: unblock the downloaded binary.
try { Unblock-File -Path $bin } catch {}

# 2: copy it into a stable per-user location.
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$target = Join-Path $InstallDir 'plaintext.exe'
Copy-Item -Path $bin -Destination $target -Force
try { Unblock-File -Path $target } catch {}
Write-Host "Installed to: $target"

# 3: put it on the user PATH (no admin rights needed).
if (-not $SkipPath) {
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($null -eq $userPath) { $userPath = '' }
    $entries = $userPath.Split(';') | Where-Object { $_ -ne '' }
    if ($entries -notcontains $InstallDir) {
        $newPath = if ($userPath -eq '') { $InstallDir } else { "$userPath;$InstallDir" }
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
        Write-Host "Added $InstallDir to your PATH."
    } else {
        Write-Host "$InstallDir is already on your PATH."
    }
    # Make it work in this window too, not just new ones.
    $env:Path = "$env:Path;$InstallDir"
}

# Verify by running the copy we just installed. If Smart App Control blocks it,
# the install itself still succeeded -- explain the block instead of erroring out.
Write-Host ''
try {
    $version = (& $target version) -join ' '
    Write-Host "Done - installed $version."
    if ($SkipPath) {
        Write-Host "Add $InstallDir to your PATH to run it as just 'plaintext'."
    } else {
        Write-Host "Open a NEW terminal, then try:  plaintext run examples\basics.pt"
    }
} catch {
    Write-Host "Installed to: $target"
    Write-Host "But Windows wouldn't let it run just now." -ForegroundColor Yellow
    if ((Get-SmartAppControlState) -ne 'Off') {
        Show-SmartAppControlHelp
    } else {
        Write-Host "Error was: $($_.Exception.Message)"
        Write-Host "Try opening a new terminal and running:  plaintext version"
    }
    exit 1
}
