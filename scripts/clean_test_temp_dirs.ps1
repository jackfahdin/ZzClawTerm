[CmdletBinding()]
param(
    [switch]$Apply
)

$ErrorActionPreference = "Stop"

$tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd(
    [System.IO.Path]::DirectorySeparatorChar
)
$uuid = "[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
$testNamePattern = "^nyaterm-.+-(?<pid>[0-9]+)-$uuid$"
$candidates = @()

foreach ($directory in Get-ChildItem -LiteralPath $tempRoot -Directory -Filter "nyaterm-*") {
    if ($directory.Name -notmatch $testNamePattern) {
        continue
    }

    $resolved = [System.IO.Path]::GetFullPath($directory.FullName)
    $parent = [System.IO.Path]::GetFullPath($directory.Parent.FullName).TrimEnd(
        [System.IO.Path]::DirectorySeparatorChar
    )
    if ($parent -ne $tempRoot) {
        throw "Refusing test directory outside the system Temp root: $resolved"
    }

    $ownerPid = [int]$Matches.pid
    if ($null -ne (Get-Process -Id $ownerPid -ErrorAction SilentlyContinue)) {
        Write-Warning "Skipping active test process $ownerPid directory: $resolved"
        continue
    }

    $candidates += $resolved
}

if (-not $Apply) {
    $candidates
    Write-Host "Found $($candidates.Count) stale NyaTerm test director$(if ($candidates.Count -eq 1) { 'y' } else { 'ies' })."
    Write-Host "Run again with -Apply to remove them."
    exit 0
}

foreach ($candidate in $candidates) {
    Remove-Item -LiteralPath $candidate -Recurse -Force
}

Write-Host "Removed $($candidates.Count) stale NyaTerm test director$(if ($candidates.Count -eq 1) { 'y' } else { 'ies' })."
