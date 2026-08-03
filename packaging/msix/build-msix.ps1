[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string] $IdentityName,

    [Parameter(Mandatory)]
    [ValidatePattern('^CN=')]
    [string] $Publisher,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string] $PublisherDisplayName,

    [string] $Version,
    [string] $Executable = 'target\release\ime-reborn.exe',
    [string] $OutputDirectory = 'dist'
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path

if ([string]::IsNullOrWhiteSpace($Version)) {
    $metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    $Version = $metadata.packages[0].version
}
if ($Version -notmatch '^\d+\.\d+\.\d+(?:\.\d+)?$') {
    throw "MSIX version must contain three or four numeric parts: $Version"
}
$parts = @($Version.Split('.') | ForEach-Object { [uint16]::Parse($_) })
while ($parts.Count -lt 4) { $parts += 0 }
$msixVersion = $parts -join '.'

$executablePath = [IO.Path]::GetFullPath((Join-Path $repoRoot $Executable))
if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
    throw "Executable not found: $executablePath. Run cargo build --release --locked first."
}

$makeAppx = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter makeappx.exe |
    Where-Object { $_.FullName -match '\\x64\\makeappx\.exe$' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName
if (-not $makeAppx) {
    throw 'makeappx.exe was not found. Install the Windows 10/11 SDK.'
}

$outputPath = [IO.Path]::GetFullPath((Join-Path $repoRoot $OutputDirectory))
$stage = Join-Path $outputPath 'msix-stage'
$assets = Join-Path $stage 'Assets'
if (Test-Path -LiteralPath $stage) {
    Remove-Item -LiteralPath $stage -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $assets | Out-Null
Copy-Item -LiteralPath $executablePath -Destination (Join-Path $stage 'ime-reborn.exe') -Force

Add-Type -AssemblyName System.Drawing
$source = [Drawing.Image]::FromFile((Join-Path $repoRoot 'assets\ime-reborn-logo.png'))
try {
    $assetSpecs = @{
        'StoreLogo.png' = @(50, 50)
        'Square44x44Logo.png' = @(44, 44)
        'Square150x150Logo.png' = @(150, 150)
        'Square310x310Logo.png' = @(310, 310)
        'Wide310x150Logo.png' = @(310, 150)
    }
    foreach ($entry in $assetSpecs.GetEnumerator()) {
        $canvas = [Drawing.Bitmap]::new($entry.Value[0], $entry.Value[1])
        try {
            $canvas.SetResolution(96, 96)
            $graphics = [Drawing.Graphics]::FromImage($canvas)
            try {
                $graphics.Clear([Drawing.Color]::Transparent)
                $graphics.CompositingQuality = [Drawing.Drawing2D.CompositingQuality]::HighQuality
                $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $side = [Math]::Min($entry.Value[0], $entry.Value[1])
                $left = [int](($entry.Value[0] - $side) / 2)
                $top = [int](($entry.Value[1] - $side) / 2)
                $graphics.DrawImage($source, $left, $top, $side, $side)
            } finally {
                $graphics.Dispose()
            }
            $canvas.Save((Join-Path $assets $entry.Key), [Drawing.Imaging.ImageFormat]::Png)
        } finally {
            $canvas.Dispose()
        }
    }
} finally {
    $source.Dispose()
}

$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'AppxManifest.xml.in') -Raw
$manifest = $manifest.Replace(
    '{{IDENTITY_NAME}}',
    [Security.SecurityElement]::Escape($IdentityName)
)
$manifest = $manifest.Replace(
    '{{PUBLISHER}}',
    [Security.SecurityElement]::Escape($Publisher)
)
$manifest = $manifest.Replace(
    '{{PUBLISHER_DISPLAY_NAME}}',
    [Security.SecurityElement]::Escape($PublisherDisplayName)
)
$manifest = $manifest.Replace('{{VERSION}}', $msixVersion)
Set-Content -LiteralPath (Join-Path $stage 'AppxManifest.xml') -Value $manifest -Encoding utf8

$baseName = "ime-reborn-v$msixVersion-windows-x86_64"
$msix = Join-Path $outputPath "$baseName.msix"
New-Item -ItemType Directory -Force -Path $outputPath | Out-Null
if (Test-Path -LiteralPath $msix) { Remove-Item -LiteralPath $msix }
& $makeAppx pack /d $stage /p $msix /o
if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed with exit code $LASTEXITCODE" }

$uploadStage = Join-Path $outputPath 'msix-upload-stage'
if (Test-Path -LiteralPath $uploadStage) {
    Remove-Item -LiteralPath $uploadStage -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $uploadStage | Out-Null
Copy-Item -LiteralPath $msix -Destination $uploadStage -Force
$pdb = [IO.Path]::ChangeExtension($executablePath, '.pdb')
if (Test-Path -LiteralPath $pdb) {
    $symbolZip = Join-Path $outputPath "$baseName-symbols.zip"
    Compress-Archive -LiteralPath $pdb -DestinationPath $symbolZip -Force
    Move-Item -LiteralPath $symbolZip -Destination (Join-Path $uploadStage "$baseName.appxsym") -Force
}
$uploadZip = Join-Path $outputPath "$baseName-upload.zip"
$upload = Join-Path $outputPath "$baseName.msixupload"
Compress-Archive -Path (Join-Path $uploadStage '*') -DestinationPath $uploadZip -Force
Move-Item -LiteralPath $uploadZip -Destination $upload -Force

Write-Output "MSIX: $msix"
Write-Output "Store upload: $upload"
