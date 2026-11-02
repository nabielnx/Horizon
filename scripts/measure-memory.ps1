param([Parameter(Mandatory=$true)][int]$ProcessId, [int]$Samples=3, [int]$IntervalSeconds=2, [string]$OutputFile)
$ErrorActionPreference='Stop'
if (-not ('HorizonRamCounters' -as [type])) {
    Add-Type -TypeDefinition @"
using System;using System.Runtime.InteropServices;
public static class HorizonRamCounters {
[DllImport("kernel32.dll",SetLastError=true)]static extern bool K32QueryWorkingSet(IntPtr process,IntPtr buffer,uint size);
public static long PrivateResident(IntPtr process){const int size=1048576;var buffer=Marshal.AllocHGlobal(size);try{if(!K32QueryWorkingSet(process,buffer,size))throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());long count=Marshal.ReadInt64(buffer),pages=0;for(int i=0;i<count;i++){long value=Marshal.ReadInt64(buffer,8+i*8);if((value&256)==0)pages++;}return pages*Environment.SystemPageSize;}finally{Marshal.FreeHGlobal(buffer);}}
}
"@
}
$process=Get-Process -Id $ProcessId
$rows=@()
for($sample=1;$sample -le $Samples;$sample++) {
    Start-Sleep -Seconds $IntervalSeconds
    $process.Refresh()
    if($process.HasExited){throw 'Measured process exited.'}
    $rows += [ordered]@{
        second=$sample*$IntervalSeconds
        workingSetMiB=[math]::Round($process.WorkingSet64/1MB,2)
        privateWorkingSetMiB=[math]::Round([HorizonRamCounters]::PrivateResident($process.Handle)/1MB,2)
        privateCommitMiB=[math]::Round($process.PrivateMemorySize64/1MB,2)
        cpuMilliseconds=[math]::Round($process.TotalProcessorTime.TotalMilliseconds,2)
        threads=$process.Threads.Count
    }
}
$json=ConvertTo-Json -InputObject $rows -Depth 4
if($OutputFile){[IO.File]::WriteAllText([IO.Path]::GetFullPath($OutputFile),$json,[Text.UTF8Encoding]::new($false))}
$json
