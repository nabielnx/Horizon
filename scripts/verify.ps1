param([string]$Executable, [switch]$SkipPreviews, [switch]$CaptureLayouts)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'toolchain.ps1')
Push-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
try {
    if (-not $Executable) { $Executable = "artifacts/release-$(Get-HorizonVersion)/Horizon.exe" }
    $exe=(Resolve-Path -LiteralPath $Executable).Path
    $startupRuns=@()
    for($run=1;$run -le 3;$run++){
        $startup=Start-Process -FilePath $exe -ArgumentList '--offscreen','--startup-probe' -PassThru -WindowStyle Hidden
        if(-not $startup.WaitForExit(20000)){$startup.Kill();throw 'Owned startup probe timed out.'}
        if($startup.ExitCode -ne 0){throw "Native startup failed: $($startup.ExitCode)"}
        $startupRuns += Get-Content -LiteralPath artifacts\startup-checks-0.3.json -Raw | ConvertFrom-Json
    }
    ConvertTo-Json -InputObject $startupRuns -Depth 6 | Set-Content -LiteralPath artifacts\startup-checks-0.3.json -Encoding utf8
    Add-Type -TypeDefinition @"
using System;using System.Runtime.InteropServices;
public static class HorizonMemoryProbe {
[DllImport("kernel32.dll",SetLastError=true)]static extern bool K32QueryWorkingSet(IntPtr process,IntPtr buffer,uint size);
public static long PrivateResident(IntPtr process){const int size=1048576;var buffer=Marshal.AllocHGlobal(size);try{if(!K32QueryWorkingSet(process,buffer,size))throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());long count=Marshal.ReadInt64(buffer),pages=0;for(int i=0;i<count;i++){long value=Marshal.ReadInt64(buffer,8+i*8);if((value&256)==0)pages++;}return pages*Environment.SystemPageSize;}finally{Marshal.FreeHGlobal(buffer);}}
}
"@
    $probeArgs=@('--offscreen','--settings','--verify')
    if ($CaptureLayouts) { $probeArgs += '--capture-layouts' }
    $probe=Start-Process -FilePath $exe -ArgumentList $probeArgs -PassThru -WindowStyle Hidden
    $samples=@()
    for($i=1;$i -le 10;$i++){
        Start-Sleep -Milliseconds 1000
        $probe.Refresh()
        if($probe.HasExited){throw 'Probe exited before measurements completed.'}
        $samples += [ordered]@{second=$i;workingSetMiB=[math]::Round($probe.WorkingSet64/1MB,2);privateBytesMiB=[math]::Round($probe.PrivateMemorySize64/1MB,2);privateWorkingSetMiB=[math]::Round([HorizonMemoryProbe]::PrivateResident($probe.Handle)/1MB,2);cpuMilliseconds=[math]::Round($probe.TotalProcessorTime.TotalMilliseconds,2)}
    }
    if(-not $probe.WaitForExit(20000)){ $probe.Kill();throw 'Owned offscreen probe timed out.' }
    if($probe.ExitCode -ne 0){throw "Native verification failed: $($probe.ExitCode)"}
    [ordered]@{version='0.3.0';exitCode=$probe.ExitCode;offscreen=$true;mediaEnabled=$false;settingsPagesRendered=7;forcedGcOrTrim=$false;samples=$samples}|ConvertTo-Json -Depth 6|Set-Content -LiteralPath artifacts\native-smoke-0.3.json -Encoding utf8
    $motion=Start-Process -FilePath $exe -ArgumentList '--offscreen','--motion-probe' -PassThru -WindowStyle Hidden
    if(-not $motion.WaitForExit(20000)){$motion.Kill();throw 'Owned motion probe timed out.'}
    if($motion.ExitCode -ne 0){throw "Native motion failed: $($motion.ExitCode)"}
    if (-not $SkipPreviews) {
    $render=Start-Process -FilePath $exe -ArgumentList '--offscreen','--render' -PassThru -WindowStyle Hidden
    if(-not $render.WaitForExit(20000)){$render.Kill();throw 'Owned render probe timed out.'}
    if($render.ExitCode -ne 0){throw "Native render failed: $($render.ExitCode)"}
    Add-Type -AssemblyName System.Drawing
    Get-ChildItem artifacts\previews -Filter *.bmp | ForEach-Object {$b=[Drawing.Bitmap]::FromFile($_.FullName);try{$b.Save([IO.Path]::ChangeExtension($_.FullName,'.png'),[Drawing.Imaging.ImageFormat]::Png)}finally{$b.Dispose()}}
    & (Join-Path $PSScriptRoot 'check-palette.ps1')
    }
    Write-Output 'Native Settings checks and renderer passed. Measurements: artifacts\native-smoke-0.3.json'
} finally {Pop-Location}
