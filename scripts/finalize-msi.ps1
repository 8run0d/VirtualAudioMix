$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$msiDir = Join-Path $repoRoot "src-tauri\target\release\bundle\msi"

if (-not (Test-Path -LiteralPath $msiDir)) {
    return
}

Remove-Item -LiteralPath (Join-Path $msiDir "dark-extract") -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -Path (Join-Path $msiDir "*.decompiled.wxs") -Force -ErrorAction SilentlyContinue

$localizedMsi = Get-ChildItem -LiteralPath $msiDir -Filter "VirtualAudioMix_*_x64_fr-FR.msi" -File |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if (-not $localizedMsi) {
    throw "MSI fr-FR introuvable dans $msiDir"
}

$finalName = $localizedMsi.Name -replace "_fr-FR\.msi$", ".msi"
$finalPath = Join-Path $msiDir $finalName

Get-ChildItem -LiteralPath $msiDir -Filter "*.msi" -File |
    Where-Object { $_.FullName -ne $localizedMsi.FullName } |
    Remove-Item -Force

if (Test-Path -LiteralPath $finalPath) {
    Remove-Item -LiteralPath $finalPath -Force
}

Move-Item -LiteralPath $localizedMsi.FullName -Destination $finalPath

Write-Host "MSI final: $finalPath"
