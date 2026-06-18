$ErrorActionPreference = "Continue"

$knownOriginalNames = @(
    "ComponentizedAudioSample.inf",
    "ComponentizedApoSample.inf",
    "ComponentizedAudioSampleExtension.inf"
)

$knownMarkers = @(
    "VAMAudio.sys",
    "Bubux Audio Driver",
    "VirtualAudioMix"
)

function Get-BadPublishedDriverNames {
    $output = & "$env:WINDIR\System32\pnputil.exe" /enum-drivers /files 2>$null
    if (-not $output) {
        $output = & "$env:WINDIR\System32\pnputil.exe" /enum-drivers 2>$null
    }

    $blocks = @()
    $current = New-Object System.Collections.Generic.List[string]

    foreach ($line in $output) {
        if ($line -match '^\s*$') {
            if ($current.Count -gt 0) {
                $blocks += ,($current -join "`n")
                $current.Clear()
            }
            continue
        }
        $current.Add($line)
    }

    if ($current.Count -gt 0) {
        $blocks += ,($current -join "`n")
    }

    foreach ($block in $blocks) {
        $published = $null
        if ($block -match '(?im)^\s*(Published Name|Nom publié|Nom publie)\s*:\s*(oem\d+\.inf)\s*$') {
            $published = $matches[2]
        }
        if (-not $published) {
            continue
        }

        $matchesKnownOriginal = $false
        foreach ($name in $knownOriginalNames) {
            if ($block -match [regex]::Escape($name)) {
                $matchesKnownOriginal = $true
                break
            }
        }

        $matchesKnownMarker = $false
        foreach ($marker in $knownMarkers) {
            if ($block -match [regex]::Escape($marker)) {
                $matchesKnownMarker = $true
                break
            }
        }

        if ($matchesKnownOriginal -or $matchesKnownMarker) {
            $published
        }
    }
}

$publishedDrivers = @(Get-BadPublishedDriverNames | Select-Object -Unique)

foreach ($publishedDriver in $publishedDrivers) {
    $process = Start-Process -FilePath "$env:WINDIR\System32\pnputil.exe" -ArgumentList @("/delete-driver", $publishedDriver, "/uninstall", "/force") -Wait -PassThru -WindowStyle Hidden
    if ($process.ExitCode -eq 3010) {
        New-Item -ItemType Directory -Force -Path "HKCU:\Software\Bruno Del piero\VirtualAudioMix" | Out-Null
        New-ItemProperty -Path "HKCU:\Software\Bruno Del piero\VirtualAudioMix" -Name "InstallerRebootSuggested" -Value 1 -PropertyType DWord -Force | Out-Null
    }
}

Remove-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run" -Name "VirtualAudioMix" -ErrorAction SilentlyContinue
