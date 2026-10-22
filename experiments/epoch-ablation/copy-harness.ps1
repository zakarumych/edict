param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('aethos-access', 'aethos-transform-diagnosis')]
    [string] $Harness
)

$experimentRoot = Split-Path -Parent $PSScriptRoot
$sourceRoot = Join-Path $experimentRoot $Harness
$destinationRoot = Join-Path $PSScriptRoot $Harness
if (-not (Test-Path -LiteralPath (Join-Path $sourceRoot 'src') -PathType Container)) {
    throw "Harness source unavailable: $sourceRoot"
}
New-Item -ItemType Directory -Path $destinationRoot -Force | Out-Null
# Copy only source inputs: no targets, result files, or Git metadata.
foreach ($name in @('Cargo.toml', 'Cargo.lock', 'README.md')) {
    $source = Join-Path $sourceRoot $name
    if (Test-Path -LiteralPath $source -PathType Leaf) {
        Copy-Item -LiteralPath $source -Destination (Join-Path $destinationRoot $name) -Force
    }
}
foreach ($name in @('src', 'benches')) {
    $source = Join-Path $sourceRoot $name
    if (Test-Path -LiteralPath $source -PathType Container) {
        $destination = Join-Path $destinationRoot $name
        New-Item -ItemType Directory -Path $destination -Force | Out-Null
        Get-ChildItem -LiteralPath $source | Copy-Item -Destination $destination -Recurse -Force
        Get-ChildItem -LiteralPath $destination -File -Recurse | ForEach-Object {
            $_.LastWriteTimeUtc = [DateTime]::UtcNow
        }
    }
}
$manifestPath = Join-Path $destinationRoot 'Cargo.toml'
$manifest = [System.IO.File]::ReadAllText($manifestPath)
$dependencyPattern = '(?m)^edict\s*=\s*\{\s*path\s*=\s*"[^"]+"\s*\}'
if ([regex]::Matches($manifest, $dependencyPattern).Count -ne 1) {
    throw 'Expected exactly one simple edict path dependency.'
}
$manifest = [regex]::Replace($manifest, $dependencyPattern, 'edict = { path = "../edict-single-thread" }')
[System.IO.File]::WriteAllText($manifestPath, $manifest)
Write-Output "Prepared SINGLE-THREAD-ONLY epoch ablation harness: $manifestPath"
