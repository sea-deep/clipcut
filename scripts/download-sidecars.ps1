# PowerShell sidecar downloader for Windows
$ErrorActionPreference = "Stop"

$BinDir = Join-Path $PSScriptRoot "..\src-tauri\binaries"
if (-not (Test-Path $BinDir)) {
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
}

$YtDlpVersion = "2026.08.19"
$YtDlpDest = Join-Path $BinDir "yt-dlp-x86_64-pc-windows-msvc.exe"
$FfmpegDest = Join-Path $BinDir "ffmpeg-x86_64-pc-windows-msvc.exe"
$FfprobeDest = Join-Path $BinDir "ffprobe-x86_64-pc-windows-msvc.exe"

# 1. yt-dlp
if (-not (Test-Path $YtDlpDest)) {
    Write-Host "Downloading yt-dlp ($YtDlpVersion)..."
    $YtDlpUrl = "https://github.com/yt-dlp/yt-dlp/releases/download/$YtDlpVersion/yt-dlp.exe"
    Invoke-WebRequest -Uri $YtDlpUrl -OutFile $YtDlpDest
    Write-Host "  -> $YtDlpDest"
} else {
    Write-Host "yt-dlp already present: $YtDlpDest"
}

# 2. ffmpeg & ffprobe
if ((-not (Test-Path $FfmpegDest)) -or (-not (Test-Path $FfprobeDest))) {
    Write-Host "Downloading FFmpeg (latest GPL)..."
    $FfmpegUrl = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
    $ZipPath = Join-Path $env:TEMP "ffmpeg-win64.zip"
    $ExtractDir = Join-Path $env:TEMP "ffmpeg-extract"
    
    Invoke-WebRequest -Uri $FfmpegUrl -OutFile $ZipPath
    Expand-Archive -Path $ZipPath -DestinationPath $ExtractDir -Force
    
    $FfmpegExe = Get-ChildItem -Path $ExtractDir -Filter "ffmpeg.exe" -Recurse | Select-Object -First 1
    $FfprobeExe = Get-ChildItem -Path $ExtractDir -Filter "ffprobe.exe" -Recurse | Select-Object -First 1
    
    Move-Item -Path $FfmpegExe.FullName -Destination $FfmpegDest -Force
    Move-Item -Path $FfprobeExe.FullName -Destination $FfprobeDest -Force
    
    Remove-Item -Path $ZipPath -Force -ErrorAction SilentlyContinue
    Remove-Item -Path $ExtractDir -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host "  -> $FfmpegDest"
    Write-Host "  -> $FfprobeDest"
} else {
    Write-Host "ffmpeg & ffprobe already present."
}

Write-Host "`nSidecars ready in $BinDir"
Get-ChildItem $BinDir
