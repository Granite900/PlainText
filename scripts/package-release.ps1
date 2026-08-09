# Build a shareable PlainText release zip for end users.
# Output: dist/plaintext-windows-x64.zip
#
# Run:  powershell -ExecutionPolicy Bypass -File scripts\package-release.ps1

$ErrorActionPreference = "Stop"
$repo = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $repo

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
Write-Host "Packaging PlainText $version ..."

Write-Host "Building release binary..."
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$vsixDir = Join-Path $repo "editors\vscode"
$vsix = Get-ChildItem (Join-Path $vsixDir "*.vsix") -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1

if (-not $vsix) {
  Write-Host "No .vsix found - packaging one..."
  Push-Location $vsixDir
  npm install
  if ($LASTEXITCODE -ne 0) {
    Pop-Location
    Write-Error "npm install failed in editors/vscode"
  }
  $readme = Get-Content README.md -Raw
  ($readme -replace '\[PlainText\]\(\.\./\.\./README\.md\)', 'PlainText') | Set-Content README.md -NoNewline
  vsce package --allow-missing-repository
  if ($LASTEXITCODE -ne 0) {
    Set-Content README.md $readme -NoNewline
    Pop-Location
    Write-Error "vsce package failed. Install with: npm install -g @vscode/vsce"
  }
  Set-Content README.md $readme -NoNewline
  Pop-Location
  $vsix = Get-ChildItem (Join-Path $vsixDir "*.vsix") |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
}

$stage = Join-Path $repo "dist\plaintext-windows-x64"
$zip = Join-Path $repo "dist\plaintext-windows-x64.zip"
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item $zip -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stage | Out-Null

Copy-Item "target\release\plaintext.exe" $stage
Copy-Item "GETTING-STARTED.md" $stage
Copy-Item "README.md" $stage
Copy-Item "docs" (Join-Path $stage "docs") -Recurse
Copy-Item "examples" (Join-Path $stage "examples") -Recurse
New-Item -ItemType Directory -Force -Path (Join-Path $stage "scripts") | Out-Null
Copy-Item "scripts\install-pt-icon.ps1" (Join-Path $stage "scripts\install-pt-icon.ps1") -Force
New-Item -ItemType Directory -Force -Path (Join-Path $stage "assets") | Out-Null
Copy-Item "assets\pt.ico" (Join-Path $stage "assets\pt.ico") -Force
if (Test-Path "assets\pt-logo.png") {
  Copy-Item "assets\pt-logo.png" (Join-Path $stage "assets\pt-logo.png")
}
Copy-Item $vsix.FullName (Join-Path $stage "plaintext-lang.vsix")

$quick = @"
PlainText $version - Windows x64

1. Add this folder to your PATH (or run .\plaintext.exe directly).
2. Try:  plaintext run examples\basics.pt
3. Read GETTING-STARTED.md and docs\language-reference.md
4. Optional VS Code:  code --install-extension plaintext-lang.vsix
5. Optional desktop icon:  powershell -ExecutionPolicy Bypass -File scripts\install-pt-icon.ps1

https://github.com/Granite900/PlainText
"@
Set-Content (Join-Path $stage "START-HERE.txt") $quick -Encoding utf8

Compress-Archive -Path $stage -DestinationPath $zip -Force
Write-Host "Created $zip"
Write-Host "Size: $([math]::Round((Get-Item $zip).Length / 1MB, 2)) MB"
