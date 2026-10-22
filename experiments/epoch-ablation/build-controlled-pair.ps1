$ErrorActionPreference = 'Stop'
$forkEpochPath = Join-Path $PSScriptRoot 'edict-single-thread\src\epoch.rs'
$sourceEpochPath = Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) 'src\epoch.rs'
$targetRoot = Join-Path $PSScriptRoot 'aethos-access\target'
$binaryRoot = Join-Path $PSScriptRoot 'binaries'
New-Item -ItemType Directory -Path $binaryRoot -Force | Out-Null
$ablatedSource = [System.IO.File]::ReadAllText($forkEpochPath)
if (-not $ablatedSource.Contains('// DIAGNOSTIC ONLY: single-thread use.')) {
    throw 'Expected diagnostic epoch source before building.'
}
$harnesses = @(
    @{ Directory = 'aethos-access'; Binary = 'aethos-access-diagnostics.exe'; Prefix = 'access' },
    @{ Directory = 'aethos-transform-diagnosis'; Binary = 'aethos-transform-diagnosis.exe'; Prefix = 'transform' }
)
try {
    foreach ($variant in @('ablated', 'baseline')) {
        if ($variant -eq 'baseline') {
            # Write bytes with a fresh timestamp so Cargo invalidates the fork.
            [System.IO.File]::WriteAllText($forkEpochPath, [System.IO.File]::ReadAllText($sourceEpochPath))
        }
        foreach ($harness in $harnesses) {
            $manifest = Join-Path $PSScriptRoot ($harness.Directory + '\Cargo.toml')
            & cargo +stable build --release --offline --manifest-path $manifest --target-dir $targetRoot
            if ($LASTEXITCODE -ne 0) { throw "Build failed: $variant $($harness.Directory)" }
            $binary = Join-Path $targetRoot ('release\' + $harness.Binary)
            $savedBinary = Join-Path $binaryRoot ($harness.Prefix + '-' + $variant + '.exe')
            Copy-Item -LiteralPath $binary -Destination $savedBinary -Force
        }
    }
} finally {
    [System.IO.File]::WriteAllText($forkEpochPath, $ablatedSource)
}
Get-ChildItem -LiteralPath $binaryRoot -Filter '*.exe' | Select-Object Name, Length
