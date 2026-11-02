$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'toolchain.ps1')
$executable = Join-Path (Split-Path -Parent $PSScriptRoot) "artifacts/release-$(Get-HorizonVersion)/Horizon.exe"
$p = Start-Process -FilePath $executable -PassThru -WindowStyle Hidden
Write-Output "Started Horizon PID: $($p.Id)"
for ($i = 1; $i -le 15; $i++) {
    Start-Sleep -Seconds 1
    if ($p.HasExited) {
        Write-Output "Process exited at second $i with exit code: $($p.ExitCode)"
        break
    }
}
if (-not $p.HasExited) {
    Write-Output "Process is stably running after 15 seconds! (PID: $($p.Id))"
}
