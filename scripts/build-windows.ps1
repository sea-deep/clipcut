# PowerShell script to set up environment and build ClipCut for Windows
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

Write-Host "=== Setting up Rust Toolchain ==="
if (-not (Get-Command rustc -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Rust MSVC..."
    Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile "rustup-init.exe"
    .\rustup-init.exe -y --default-toolchain stable --profile minimal
    Remove-Item "rustup-init.exe" -Force
}
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
rustc --version
cargo --version

Write-Host "`n=== Setting up Node.js ==="
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Node.js LTS via Chocolatey..."
    choco install nodejs-lts -y --no-progress
    $env:PATH = "$env:ProgramFiles\nodejs;$env:PATH"
}
node --version
npm --version

Write-Host "`n=== Downloading Bundled Sidecars (yt-dlp, FFmpeg) ==="
& "$PSScriptRoot\download-sidecars.ps1"

Write-Host "`n=== Installing NPM Dependencies ==="
npm ci

Write-Host "`n=== Building ClipCut Windows Installer (NSIS) ==="
npm run tauri build -- --bundles nsis

Write-Host "`n=== Windows Build Finished Successfully ==="
Get-ChildItem -Path "src-tauri\target\release\bundle\nsis" -Filter "*.exe" -ErrorAction SilentlyContinue
