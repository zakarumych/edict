$ErrorActionPreference = 'Stop'
$ablationRoot = (Resolve-Path -LiteralPath $PSScriptRoot).ProviderPath
$sourceRoot = (Resolve-Path -LiteralPath (Join-Path $ablationRoot '..\..')).ProviderPath
$forkRoot = [System.IO.Path]::GetFullPath((Join-Path $ablationRoot 'edict-single-thread'))
$allowedPrefix = $ablationRoot.TrimEnd('\') + '\'
if (-not $forkRoot.StartsWith($allowedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Fork destination must stay inside the ablation directory.'
}
if ((Get-Item -LiteralPath $ablationRoot).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw 'Ablation directory must not be a symbolic link or junction.'
}

$sourceFiles = @()
foreach ($folder in @('src', 'proc', 'proc-lib', 'examples')) {
    $directory = Join-Path $sourceRoot $folder
    $items = @(Get-ChildItem -LiteralPath $directory -Force -Recurse)
    if (@($items | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }).Count -gt 0) {
        throw "Source tree contains a symbolic link or junction: $directory"
    }
    $sourceFiles += $items | Where-Object {
        -not $_.PSIsContainer -and $_.FullName -notmatch '[\\/](target|\.git)[\\/]'
    }
}
$sourceFiles += Get-Item -LiteralPath (Join-Path $sourceRoot 'Cargo.toml')
$epochSource = [IO.File]::ReadAllText((Join-Path $sourceRoot 'src\epoch.rs'))
$oldLine = '        let old = self.value.fetch_add(1, Ordering::Relaxed);'
if ([regex]::Matches($epochSource, [regex]::Escape($oldLine)).Count -ne 1) {
    throw 'Expected exactly one EpochCounter::next atomic fetch_add site.'
}

# Preserve any existing snapshot without deletion. Both resolved move paths
# are direct children of this experiment directory.
if (Test-Path -LiteralPath $forkRoot) {
    $existing = Get-Item -LiteralPath $forkRoot -Force
    if (-not $existing.PSIsContainer -or ($existing.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'Existing fork must be an ordinary directory.'
    }
    $archiveRoot = [IO.Path]::GetFullPath((Join-Path $ablationRoot ('edict-single-thread-backup-' + [Guid]::NewGuid().ToString('N'))))
    if (-not $archiveRoot.StartsWith($allowedPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Backup destination must stay inside the ablation directory.'
    }
    Move-Item -LiteralPath $forkRoot -Destination $archiveRoot
    Write-Output "Preserved existing fork: $archiveRoot"
}
New-Item -ItemType Directory -Path $forkRoot | Out-Null
foreach ($sourceFile in $sourceFiles) {
    $relative = $sourceFile.FullName.Substring($sourceRoot.Length + 1)
    $destination = [IO.Path]::GetFullPath((Join-Path $forkRoot $relative))
    if (-not $destination.StartsWith($forkRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "Source destination escapes fork: $relative"
    }
    New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
    Copy-Item -LiteralPath $sourceFile.FullName -Destination $destination
    (Get-Item -LiteralPath $destination).LastWriteTimeUtc = [DateTime]::UtcNow
}
$newLines = @'
        // DIAGNOSTIC ONLY: single-thread use. Concurrent callers can lose epochs.
        let old = self.value.load(Ordering::Relaxed);
        self.value.store(old.wrapping_add(1), Ordering::Relaxed);
'@
$epochDestination = Join-Path $forkRoot 'src\epoch.rs'
[IO.File]::WriteAllText($epochDestination, $epochSource.Replace($oldLine, $newLines.TrimEnd()))
foreach ($sourceFile in $sourceFiles) {
    $relative = $sourceFile.FullName.Substring($sourceRoot.Length + 1)
    if ($relative -eq 'src\epoch.rs') { continue }
    if ((Get-FileHash -LiteralPath $sourceFile.FullName).Hash -ne
        (Get-FileHash -LiteralPath (Join-Path $forkRoot $relative)).Hash) {
        throw "Snapshot differs from source: $relative"
    }
}
Write-Output 'Prepared SINGLE-THREAD-ONLY fork; only src/epoch.rs differs from copied source.'
