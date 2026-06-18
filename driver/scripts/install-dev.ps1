param(
    [string]$PackagePath = "$PSScriptRoot\..\Windows-driver-samples\audio\sysvad\x64\Release\package",
    [string]$HardwareId = "Root\VAM_ComponentizedAudioSample"
)

$ErrorActionPreference = "Stop"
$logPath = Join-Path $PSScriptRoot "install-dev.log"
$devcon = "C:\Program Files (x86)\Windows Kits\10\Tools\10.0.26100.0\x64\devcon.exe"

Start-Transcript -Path $logPath -Append | Out-Null

function Assert-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        throw "Ce script doit être exécuté dans un shell administrateur."
    }
}

function Assert-File {
    param([string]$Path)
    if (-not (Test-Path $Path)) {
        throw "Fichier introuvable: $Path"
    }
}

function Test-RebootRequiredOutput {
    param([string[]]$Output)
    ($Output -join "`n") -match "redémarrage|redemarrage|reboot|restart"
}

try {
    Assert-Admin
    $resolvedPackagePath = Resolve-Path $PackagePath

    Assert-File (Join-Path $resolvedPackagePath "ComponentizedAudioSample.inf")
    Assert-File (Join-Path $resolvedPackagePath "ComponentizedApoSample.inf")
    Assert-File (Join-Path $resolvedPackagePath "ComponentizedAudioSampleExtension.inf")
    Assert-File (Join-Path $resolvedPackagePath "sysvad.cat")
    Assert-File (Join-Path $resolvedPackagePath "VAMAudio.sys")

    $testSigning = (bcdedit /enum) -match "testsigning\s+Yes"
    if (-not $testSigning) {
        throw "Le mode testsigning n'est pas actif. Exécuter: bcdedit /set testsigning on puis redémarrer."
    }

    $certificatePath = Resolve-Path (Join-Path $resolvedPackagePath "..\package.cer")
    certutil.exe -addstore -f Root $certificatePath | Out-Host
    certutil.exe -addstore -f TrustedPublisher $certificatePath | Out-Host

    Push-Location $resolvedPackagePath
    try {
        if (Test-Path $devcon) {
            $baseOutput = & $devcon install ComponentizedAudioSample.inf $HardwareId 2>&1
            $baseOutput | Out-Host
            $baseText = $baseOutput -join "`n"
            $devconSucceeded = $LASTEXITCODE -eq 0 -or $baseText -match "Drivers installed successfully|Les pilotes ont été installés correctement"
            if (-not $devconSucceeded) {
                throw "devcon install a échoué avec le code $LASTEXITCODE."
            }
            if (Test-RebootRequiredOutput $baseOutput) {
                throw "Installation base interrompue: Windows demande un redémarrage."
            }
        } else {
            throw "devcon.exe introuvable: $devcon"
        }

        $apoOutput = pnputil.exe /add-driver ComponentizedApoSample.inf /install 2>&1
        $apoOutput | Out-Host
        if ($LASTEXITCODE -ne 0) {
            throw "Installation APO échouée avec le code $LASTEXITCODE."
        }
        if (Test-RebootRequiredOutput $apoOutput) {
            throw "Installation APO interrompue: Windows demande un redémarrage."
        }

        $extensionOutput = pnputil.exe /add-driver ComponentizedAudioSampleExtension.inf /install 2>&1
        $extensionOutput | Out-Host
        if ($LASTEXITCODE -ne 0) {
            throw "Installation extension échouée avec le code $LASTEXITCODE."
        }
        if (Test-RebootRequiredOutput $extensionOutput) {
            throw "Installation extension terminée mais Windows demande un redémarrage."
        }
    } finally {
        Pop-Location
    }

    Get-PnpDevice | Where-Object {
        $_.FriendlyName -match "Bubux|BAD|VirtualAudioMix|VAM|Audio Proxy" -or $_.InstanceId -match "ROOT\\MEDIA|BAD|VAM"
    } | Select-Object Status, Class, FriendlyName, InstanceId | Format-Table -AutoSize
} finally {
    Stop-Transcript | Out-Null
}
