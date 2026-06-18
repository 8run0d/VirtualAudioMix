$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$bundleDir = Join-Path $repoRoot "src-tauri\target\release\bundle"

foreach ($name in @("msi", "nsis")) {
    $target = Join-Path $bundleDir $name
    if (Test-Path -LiteralPath $target) {
        Remove-Item -LiteralPath $target -Recurse -Force
    }
}
