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

# Verify by running the copy we just installed.
Write-Host ''
$version = (& $target version) -join ' '
Write-Host "Done - installed $version."
if ($SkipPath) {
    Write-Host "Add $InstallDir to your PATH to run it as just 'plaintext'."
} else {
    Write-Host "Open a NEW terminal, then try:  plaintext run examples\basics.pt"
}
