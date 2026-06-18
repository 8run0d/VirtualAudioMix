$ErrorActionPreference = "Stop"

function Invoke-PnPUtil {
    param(
        [Parameter(Mandatory = $true)]
        [string[]] $Arguments,
        [switch] $AllowRebootCode
    )

    $process = Start-Process -FilePath "$env:WINDIR\System32\pnputil.exe" -ArgumentList $Arguments -Wait -PassThru -WindowStyle Hidden
    if ($process.ExitCode -eq 0) {
        return
    }
    if ($AllowRebootCode -and $process.ExitCode -eq 3010) {
        New-Item -ItemType Directory -Force -Path "HKCU:\Software\Bruno Del piero\VirtualAudioMix" | Out-Null
        New-ItemProperty -Path "HKCU:\Software\Bruno Del piero\VirtualAudioMix" -Name "InstallerRebootSuggested" -Value 1 -PropertyType DWord -Force | Out-Null
        return
    }
    throw "pnputil failed with exit code $($process.ExitCode): $($Arguments -join ' ')"
}

function Test-BadDriverAlreadyPresent {
    $output = & "$env:WINDIR\System32\pnputil.exe" /enum-drivers /files 2>$null
    if (-not $output) {
        $output = & "$env:WINDIR\System32\pnputil.exe" /enum-drivers 2>$null
    }

    $text = $output -join "`n"
    return ($text -match "ComponentizedAudioSample\.inf") -or
        ($text -match "VAMAudio\.sys") -or
        ($text -match "Bubux Audio Driver")
}

$resourcesDir = Split-Path -Parent $PSScriptRoot
$driverDir = Join-Path $resourcesDir "driver\x64"
$primaryInf = Join-Path $driverDir "ComponentizedAudioSample.inf"
$apoInf = Join-Path $driverDir "ComponentizedApoSample.inf"
$extensionInf = Join-Path $driverDir "ComponentizedAudioSampleExtension.inf"

if (-not (Test-Path -LiteralPath $primaryInf)) {
    throw "BAD driver package not found: $primaryInf"
}

$prefsKey = "HKCU:\Software\Bruno Del piero\VirtualAudioMix"
New-Item -ItemType Directory -Force -Path $prefsKey | Out-Null
New-ItemProperty -Path $prefsKey -Name "InstallerBadAlreadyPresent" -Value ($(if (Test-BadDriverAlreadyPresent) { 1 } else { 0 })) -PropertyType DWord -Force | Out-Null

Invoke-PnPUtil -Arguments @("/add-driver", $primaryInf, "/install") -AllowRebootCode

if (Test-Path -LiteralPath $apoInf) {
    Invoke-PnPUtil -Arguments @("/add-driver", $apoInf, "/install") -AllowRebootCode
}

if (Test-Path -LiteralPath $extensionInf) {
    Invoke-PnPUtil -Arguments @("/add-driver", $extensionInf, "/install") -AllowRebootCode
}
