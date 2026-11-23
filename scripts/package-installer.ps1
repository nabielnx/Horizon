# Run with Windows PowerShell after building the portable release.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'toolchain.ps1')
$root = Split-Path -Parent $PSScriptRoot
$version = Get-HorizonVersion
$package = Join-Path $root "artifacts/release-$version"
$output = Join-Path $root "artifacts/Horizon-$version-win-x64.msi"
$target = Join-Path $root 'target'
$stage = Join-Path $target ('installer-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage -Force | Out-Null

function Import-Table($name, $columns, $types, $key, $rows) {
    (@($columns, $types, "$name~$key") + $rows) |
        ForEach-Object { $_.Replace('~', [string][char]9) } |
        Set-Content -LiteralPath (Join-Path $stage "$name.idt") -Encoding ascii
    $database.Import($stage, "$name.idt")
}

$engine = $null
$database = $null
$summary = $null
$stream = $null
$view = $null
try {
    $payload = @(
        @{ Component = 'App'; Folder = ''; Names = @('Horizon.exe','README.md','LICENSE','PRIVACY.md','THIRD_PARTY_NOTICES.md') }
        @{ Component = 'Fonts'; Folder = 'fonts'; Names = @('OFL.txt','PlusJakartaSans-Bold.ttf','PlusJakartaSans-Medium.ttf','PlusJakartaSans-SemiBold.ttf','Poppins-Bold.ttf','Poppins-ExtraLight.ttf','Poppins-OFL.txt','Poppins-Regular.ttf','Poppins-SemiBold.ttf') }
        @{ Component = 'Licenses'; Folder = 'licenses'; Names = @('phosphor.txt') }
    )
    $ddf = [Collections.Generic.List[string]]::new()
    foreach ($line in @('.Set CabinetNameTemplate=Horizon.cab', '.Set DiskDirectoryTemplate=.', '.Set MaxDiskSize=0', '.Set Cabinet=ON', '.Set Compress=ON', '.Set CompressionType=MSZIP')) {
        $ddf.Add($line)
    }
    $fileRows = [Collections.Generic.List[string]]::new()
    $sequence = 0
    foreach ($group in $payload) {
        foreach ($name in $group.Names) {
            $folder = if ($group.Folder) { Join-Path $package $group.Folder } else { $package }
            $file = Get-Item -LiteralPath (Join-Path $folder $name)
            $sequence++
            $id = "Payload$sequence"
            $short = 'HZ{0:D6}{1}' -f $sequence, $file.Extension
            $ddf.Add(('"{0}" {1}' -f $file.FullName, $id))
            $fileRows.Add("$id~$($group.Component)~$short|$name~$($file.Length)~~~16384~$sequence")
        }
    }
    Set-Content -LiteralPath (Join-Path $stage 'payload.ddf') -Value $ddf -Encoding Default
    Push-Location -LiteralPath $stage
    try {
        & makecab.exe /F payload.ddf | Out-Null
        if ($LASTEXITCODE) { throw "makecab exited with code $LASTEXITCODE" }
    } finally { Pop-Location }

    $msi = Join-Path $stage 'Horizon.msi'
    $engine = New-Object -ComObject WindowsInstaller.Installer
    $database = $engine.OpenDatabase($msi, 3)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $digest = $sha.ComputeHash([Text.Encoding]::UTF8.GetBytes("nabielnx.Horizon.MSI.$version"))
        $productCode = [guid]::new([byte[]]($digest[0..15])).ToString('B').ToUpperInvariant()
    } finally { $sha.Dispose() }

    Import-Table Property 'Property~Value' 's72~l0' 'Property' @(
        'ProductName~Horizon', "ProductVersion~$version", "ProductCode~$productCode",
        'Manufacturer~Horizon', 'ProductLanguage~1033', 'ARPNOMODIFY~1'
    )
    Import-Table Directory 'Directory~Directory_Parent~DefaultDir' 's72~S72~l255' 'Directory' @(
        'TARGETDIR~~SourceDir', 'LocalAppDataFolder~TARGETDIR~.',
        'LocalPrograms~LocalAppDataFolder~Programs', 'INSTALLFOLDER~LocalPrograms~Horizon',
        'HorizonFonts~INSTALLFOLDER~fonts', 'HorizonLicenses~INSTALLFOLDER~licenses',
        'ProgramMenuFolder~TARGETDIR~.'
    )
    Import-Table Component 'Component~ComponentId~Directory_~Attributes~Condition~KeyPath' 's72~S38~s72~i2~S255~S72' 'Component' @(
        'App~{C7D7C747-64A8-49EE-A3F5-6B8943FCD235}~INSTALLFOLDER~260~~AppKey',
        'Fonts~{EEF21BAD-28C5-4C4A-B645-65D3314B0523}~HorizonFonts~260~~FontsKey',
        'Licenses~{46F1B1E0-5CE3-49F0-80E2-EF3DD716C1F8}~HorizonLicenses~260~~LicensesKey'
    )
    Import-Table Feature 'Feature~Feature_Parent~Title~Description~Display~Level~Directory_~Attributes' 's38~S38~L64~L255~I2~i2~S72~i2' 'Feature' @(
        'Main~~Horizon~~1~1~INSTALLFOLDER~0'
    )
    Import-Table FeatureComponents 'Feature_~Component_' 's38~s72' 'Feature_~Component_' @(
        'Main~App', 'Main~Fonts', 'Main~Licenses'
    )
    Import-Table Registry 'Registry~Root~Key~Name~Value~Component_' 's72~i2~l255~L255~L0~s72' 'Registry' @(
        'AppKey~1~Software\Horizon\Installer~App~[INSTALLFOLDER]~App',
        'FontsKey~1~Software\Horizon\Installer~Fonts~[HorizonFonts]~Fonts',
        'LicensesKey~1~Software\Horizon\Installer~Licenses~[HorizonLicenses]~Licenses'
    )
    Import-Table File 'File~Component_~FileName~FileSize~Version~Language~Attributes~Sequence' 's72~s72~l255~i4~S72~S20~I2~i2' 'File' ($fileRows.ToArray())
    Import-Table Media 'DiskId~LastSequence~DiskPrompt~Cabinet~VolumeLabel~Source' 'i2~i4~L64~S255~S32~S72' 'DiskId' @(
        "1~$sequence~~#Horizon.cab~~"
    )
    Import-Table Shortcut 'Shortcut~Directory_~Name~Component_~Target~Arguments~Description~Hotkey~Icon_~IconIndex~ShowCmd~WkDir~DisplayResourceDLL~DisplayResourceId~DescriptionResourceDLL~DescriptionResourceId' 's72~s72~l128~s72~s72~S255~L255~I2~S72~I2~I2~S72~S255~I2~S255~I2' 'Shortcut' @(
        'HorizonMenu~ProgramMenuFolder~Horizon~App~[#Payload1]~~Open Horizon~~~~1~INSTALLFOLDER~~~~'
    )
    $sequenceColumns = 'Action~Condition~Sequence'
    $sequenceTypes = 's72~S255~I2'
    $costing = @('CostInitialize~~800','FileCost~~900','CostFinalize~~1000')
    $transaction = @('InstallValidate~~1400','InstallInitialize~~1500')
    Import-Table InstallUISequence $sequenceColumns $sequenceTypes 'Action' ($costing + 'ExecuteAction~~1300')
    Import-Table InstallExecuteSequence $sequenceColumns $sequenceTypes 'Action' ($costing + $transaction + @(
        'ProcessComponents~~1600','UnpublishFeatures~~1800','RemoveRegistryValues~~2600',
        'RemoveShortcuts~~3200','RemoveFiles~~3500','InstallFiles~~4000','CreateShortcuts~~4500',
        'WriteRegistryValues~~5000','RegisterUser~~6000','RegisterProduct~~6100',
        'PublishFeatures~~6300','PublishProduct~~6400','InstallFinalize~~6600'
    ))
    Import-Table AdminExecuteSequence $sequenceColumns $sequenceTypes 'Action' ($costing + $transaction + @(
        'InstallAdminPackage~~3900','InstallFiles~~4000','InstallFinalize~~6600'
    ))

    $stream = $engine.CreateRecord(2)
    $stream.StringData(1) = 'Horizon.cab'
    $stream.SetStream(2, (Join-Path $stage 'Horizon.cab'))
    $view = $database.OpenView(('INSERT INTO {0}_Streams{0} ({0}Name{0}, {0}Data{0}) VALUES (?, ?)' -f [char]96))
    $view.Execute($stream)
    $view.Close()

    $summary = $database.SummaryInformation(12)
    $summary.Property(1) = 1252
    $summary.Property(2) = 'Horizon Installer'
    $summary.Property(3) = "Horizon $version"
    $summary.Property(4) = 'Horizon'
    $summary.Property(7) = 'x64;1033'
    $summary.Property(9) = [guid]::NewGuid().ToString('B').ToUpperInvariant()
    $summary.Property(14) = 500
    $summary.Property(15) = 10 # Compressed files, per-user privileges.
    $summary.Persist()
    $database.Commit()

    Copy-Item -LiteralPath $msi -Destination $output -Force
    Get-Item -LiteralPath $output | Select-Object FullName, Length
} finally {
    foreach ($object in @($summary, $view, $stream, $database, $engine)) {
        if ($null -ne $object) {
            [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($object)
        }
    }
    $resolved = (Resolve-Path -LiteralPath $stage).Path
    $prefix = (Resolve-Path -LiteralPath $target).Path + [IO.Path]::DirectorySeparatorChar
    if ($resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolved) -match '^installer-[0-9a-f]{32}$') {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
