param(
    [switch]$DeleteDriverStore
)

$ErrorActionPreference = "Stop"
$logPath = Join-Path $PSScriptRoot "uninstall-dev.log"

Start-Transcript -Path $logPath -Append | Out-Null

function Assert-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Ce script doit être exécuté dans un shell administrateur."
    }
}

function Invoke-PnpUtil {
    param([string[]]$Arguments)
    $output = pnputil.exe @Arguments 2>&1
    $output | Out-Host
    [pscustomobject]@{
        ExitCode = $LASTEXITCODE
        Output = $output
        RebootRequired = (($output -join "`n") -match "redémarrage|redemarrage|reboot|restart")
    }
}

function Get-BadOrVamDevice {
    Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {
        $_.FriendlyName -match "Bubux|BAD|VirtualAudioMix|VAM|SYSVAD|Audio Proxy" -or
        $_.InstanceId -match "BAD|VAM|SYSVAD"
    }
}

try {
    Assert-Admin

    $mediaDevices = Get-BadOrVamDevice | Where-Object { $_.Class -eq "MEDIA" }
    foreach ($device in $mediaDevices) {
        $result = Invoke-PnpUtil -Arguments @("/remove-device", $device.InstanceId)
        if ($result.RebootRequired) {
            throw "Suppression interrompue: Windows demande un redémarrage avant de continuer."
        }
        if ($result.ExitCode -ne 0) {
            throw "Suppression impossible pour $($device.InstanceId). Code: $($result.ExitCode)"
        }
    }

    $childDevices = Get-BadOrVamDevice | Where-Object { $_.Class -in @("AudioEndpoint", "AudioProcessingObject") -or $_.Status -eq "Unknown" }
    foreach ($device in $childDevices) {
        $result = Invoke-PnpUtil -Arguments @("/remove-device", $device.InstanceId)
        if ($result.RebootRequired) {
            throw "Suppression interrompue: Windows demande un redémarrage avant de continuer."
        }
    }

    if ($DeleteDriverStore) {
        $raw = pnputil.exe /enum-drivers
        $blocks = @()
        $current = @()
        foreach ($line in $raw) {
            if ($line -match "^\s*$") {
                if ($current.Count) {
                    $blocks += ,($current -join "`n")
                    $current = @()
                }
            } else {
                $current += $line
            }
        }
        if ($current.Count) {
            $blocks += ,($current -join "`n")
        }

        $driverPackages = $blocks | Where-Object {
            $_ -match "componentizedaudiosample|componentizedaposample|Bubux|BAD|VirtualAudioMix|TODO-Set-Provider"
        } | ForEach-Object {
            if ($_ -match "oem\d+\.inf") {
                $Matches[0]
            }
        } | Sort-Object -Unique

        foreach ($driverPackage in $driverPackages) {
            $result = Invoke-PnpUtil -Arguments @("/delete-driver", $driverPackage, "/uninstall", "/force")
            if ($result.RebootRequired) {
                throw "Suppression DriverStore interrompue: Windows demande un redémarrage."
            }
        }
    }

    Get-BadOrVamDevice | Select-Object Status, Class, FriendlyName, InstanceId | Format-Table -AutoSize
} finally {
    Stop-Transcript | Out-Null
}
