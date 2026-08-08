# Associate .pt files with the PlainText icon in Windows Explorer / Desktop.
# Run:  powershell -ExecutionPolicy Bypass -File scripts\install-pt-icon.ps1

$ErrorActionPreference = "Stop"

$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
$srcIco = Join-Path $repo "assets\pt.ico"
if (-not (Test-Path $srcIco)) {
  Write-Error "Missing icon: $srcIco"
}

# Stable copy so the icon still works if you move/rename the repo folder.
$stableDir = Join-Path $env:LOCALAPPDATA "PlainText"
New-Item -ItemType Directory -Force -Path $stableDir | Out-Null
$ico = Join-Path $stableDir "pt.ico"
Copy-Item $srcIco $ico -Force

$progId = "PlainText.pt"

New-Item -Path "HKCU:\Software\Classes\.pt" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\.pt" -Name "(default)" -Value $progId
New-Item -Path "HKCU:\Software\Classes\.pt\OpenWithProgids" -Force | Out-Null
New-ItemProperty -Path "HKCU:\Software\Classes\.pt\OpenWithProgids" -Name $progId -PropertyType None -Force -ErrorAction SilentlyContinue | Out-Null

New-Item -Path "HKCU:\Software\Classes\$progId" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\$progId" -Name "(default)" -Value "PlainText Source File"
New-Item -Path "HKCU:\Software\Classes\$progId\DefaultIcon" -Force | Out-Null
# Quoted path is required for reliable Explorer icon loading.
Set-ItemProperty -Path "HKCU:\Software\Classes\$progId\DefaultIcon" -Name "(default)" -Value "`"$ico`",0"

$plaintext = $null
$cmd = Get-Command plaintext -ErrorAction SilentlyContinue
if ($cmd) { $plaintext = $cmd.Source }
if (-not $plaintext) {
  $candidate = Join-Path $env:USERPROFILE ".cargo\bin\plaintext.exe"
  if (Test-Path $candidate) { $plaintext = $candidate }
}
if ($plaintext) {
  New-Item -Path "HKCU:\Software\Classes\$progId\shell\open\command" -Force | Out-Null
  Set-ItemProperty -Path "HKCU:\Software\Classes\$progId\shell\open\command" -Name "(default)" -Value "`"$plaintext`" run `"%1`""
  Write-Host "Open-with set to: $plaintext"
} else {
  Write-Host "plaintext.exe not found - icon only (no double-click handler)."
}

New-Item -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.pt\OpenWithProgids" -Force | Out-Null
New-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.pt\OpenWithProgids" -Name $progId -PropertyType None -Force -ErrorAction SilentlyContinue | Out-Null
Remove-Item "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.pt\UserChoice" -Recurse -Force -ErrorAction SilentlyContinue

$code = @'
using System;
using System.Runtime.InteropServices;
public class PtShell {
  [DllImport("shell32.dll")] public static extern void SHChangeNotify(int ev, uint flags, IntPtr a, IntPtr b);
}
'@
Add-Type -TypeDefinition $code -ErrorAction SilentlyContinue
[PtShell]::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

Write-Host "Registered .pt icon -> $ico"
Write-Host "If the desktop still shows a generic icon, restart Explorer or sign out/in."
