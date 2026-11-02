$ErrorActionPreference='Stop'
Push-Location -LiteralPath (Split-Path -Parent $PSScriptRoot)
try{
    Add-Type -AssemblyName System.Drawing
    $colors=@()
    for($i=0;$i-lt6;$i++){
        $image=[Drawing.Bitmap]::FromFile((Join-Path $PWD "artifacts/previews/page-$i.png"))
        try{$colors+=('{0:X8}' -f $image.GetPixel(64,16).ToArgb())}finally{$image.Dispose()}
    }
    if(@($colors|Select-Object -Unique).Count-ne1){throw 'Pages do not share the same backdrop.'}
    $paused=[Drawing.Bitmap]::FromFile((Join-Path $PWD 'artifacts/previews/paused.png'))
    try{$top=$paused.GetPixel(0,0);$stopped=('{0:X8}' -f $paused.GetPixel(64,16).ToArgb())}finally{$paused.Dispose()}
    if($stopped-eq$colors[0]){throw 'Paused music did not restore the base palette.'}
    if([math]::Abs($top.R-46)-gt1-or[math]::Abs($top.G-49)-gt1-or[math]::Abs($top.B-53)-gt1){throw 'Paused backdrop does not match dark titanium.'}
    [ordered]@{sameAcrossFivePages=$true;playingSamples=$colors;pausedSample=$stopped;pausedDarkTitanium=$true;usesFixtureCover=$true}|ConvertTo-Json|Set-Content -LiteralPath artifacts\palette-checks-0.3.json -Encoding utf8
    Write-Output 'Shared palette across five pages and paused dark titanium verified.'
}finally{Pop-Location}
