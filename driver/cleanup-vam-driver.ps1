$ErrorActionPreference = "Continue"

$repoRoot = "C:\Users\brunod\Documents\Code\VirtualAudioMix"
$logPath = Join-Path $repoRoot "driver\cleanup-vam-driver.log"
$packagePath = Join-Path $repoRoot "driver\Windows-driver-samples\audio\sysvad\x64\Release\package"
$devcon = "C:\Program Files (x86)\Windows Kits\10\Tools\10.0.26100.0\x64\devcon.exe"
$taskName = "BAD/VAM Driver Cleanup"
$oldDriverPackages = @(
    "oem68.inf",
    "oem81.inf",
    "oem83.inf",
    "oem88.inf",
    "oem92.inf",
    "oem95.inf",
    "oem96.inf",
    "oem97.inf",
    "oem98.inf",
    "oem99.inf"
)

Start-Transcript -Path $logPath -Append | Out-Null

function Write-Step {
    param([string]$Message)
    Write-Host "[$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')] $Message"
}

function Get-BadOrVamDevices {
    Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {
        $hardwareIds = (Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName "DEVPKEY_Device_HardwareIds" -ErrorAction SilentlyContinue).Data -join ","
        $_.FriendlyName -match "VirtualAudioMix|SYSVAD|Audio Proxy" -or
        $_.InstanceId -match "BAD|VAM|SYSVAD|ROOT\\MEDIA" -or
        $hardwareIds -match "BAD|VAM|SYSVAD|VEN_SMPL&CID_APO"
    }
}

function Remove-DeviceIfPresent {
    param([string]$InstanceId)

    if ([string]::IsNullOrWhiteSpace($InstanceId)) {
        return
    }

    Write-Step "Removing device: $InstanceId"
    pnputil.exe /remove-device "$InstanceId"
    if ($LASTEXITCODE -ne 0 -and (Test-Path $devcon)) {
        & $devcon remove "@$InstanceId"
    }
}

Write-Step "Waiting for PnP to settle"
Start-Sleep -Seconds 25

Write-Step "Initial BAD/VAM/SYSVAD devices"
Get-BadOrVamDevices | Select-Object Status, Class, FriendlyName, InstanceId | Format-Table -AutoSize

$driverMediaDevices = Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {
    $_.Class -eq "MEDIA" -and (
        $_.FriendlyName -match "VirtualAudioMix|SYSVAD" -or
        ((Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName "DEVPKEY_Device_HardwareIds" -ErrorAction SilentlyContinue).Data -join ",") -match "BAD|VAM|SYSVAD"
    )
}

foreach ($device in $driverMediaDevices) {
    Remove-DeviceIfPresent $device.InstanceId
}

Start-Sleep -Seconds 5

$activeMediaDevices = Get-PnpDevice -ErrorAction SilentlyContinue | Where-Object {
    $_.Class -eq "MEDIA" -and
    $_.Status -eq "OK" -and
    ($_.FriendlyName -match "VirtualAudioMix|SYSVAD" -or
    (((Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName "DEVPKEY_Device_HardwareIds" -ErrorAction SilentlyContinue).Data -join ",") -match "BAD|VAM|SYSVAD"))
}

if ($activeMediaDevices) {
    Write-Step "Abort: a BAD/VAM/SYSVAD MEDIA device is still active after removal attempt"
    $activeMediaDevices | Select-Object Status, Class, FriendlyName, InstanceId | Format-Table -AutoSize
    Stop-Transcript | Out-Null
    exit 2
}

$staleChildren = Get-BadOrVamDevices | Where-Object {
    $_.Class -in @("AudioEndpoint", "AudioProcessingObject") -or $_.Status -eq "Unknown"
}

foreach ($device in $staleChildren) {
    Remove-DeviceIfPresent $device.InstanceId
}

Write-Step "Deleting old DriverStore packages"
foreach ($driverPackage in $oldDriverPackages) {
    pnputil.exe /delete-driver $driverPackage /uninstall /force
}

Write-Step "Installing corrected Release package"
Push-Location $packagePath
try {
    if (Test-Path $devcon) {
        & $devcon install ComponentizedAudioSample.inf "Root\BAD_ComponentizedAudioSample"
    } else {
        pnputil.exe /add-driver ComponentizedAudioSample.inf /install
    }
    $baseExitCode = $LASTEXITCODE
    Write-Step "Base INF install exit code: $baseExitCode"

    pnputil.exe /add-driver ComponentizedApoSample.inf /install
    $apoExitCode = $LASTEXITCODE
    Write-Step "APO INF install exit code: $apoExitCode"

    pnputil.exe /add-driver ComponentizedAudioSampleExtension.inf /install
    $extensionExitCode = $LASTEXITCODE
    Write-Step "Extension INF install exit code: $extensionExitCode"
} finally {
    Pop-Location
}

Start-Sleep -Seconds 8

Write-Step "Final BAD/VAM/SYSVAD devices"
Get-BadOrVamDevices | Select-Object Status, Class, FriendlyName, InstanceId | Format-Table -AutoSize

Write-Step "Deleting scheduled task"
schtasks.exe /Delete /TN $taskName /F

Stop-Transcript | Out-Null
