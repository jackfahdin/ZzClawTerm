# Fetches the pinned ZzXsrv slim build and exports the tree that
# package_native.py bundles into Windows x64 packages.
#
# Version and digest are pinned together; bump both or the download is
# rejected. The slim edition keeps the fonts and drops only what a managed
# `vcxsrv -multiwindow -clipboard` launch never loads (Mesa software GL,
# plink, xlaunch and the demo clients).
$ErrorActionPreference = 'Stop'

$version = if ($env:ZZCLAWTERM_VCXSRV_VERSION) {
    $env:ZZCLAWTERM_VCXSRV_VERSION
} else {
    '21.1.16.1'
}
$expectedHash = if ($env:ZZCLAWTERM_VCXSRV_SHA256) {
    $env:ZZCLAWTERM_VCXSRV_SHA256
} else {
    '9aa4dc87a98d07abe1ea44954974300823fddf4bf9344be6d18da5ddd2b99711'
}

Write-Output "::group::Install ZzXsrv $version"

$archiveName = "zzxsrv-$version-x64-slim-portable.zip"
$downloadUrl = "https://github.com/jackfahdin/ZzXsrv/releases/download/v$version/$archiveName"
$workDir = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$archivePath = Join-Path $workDir $archiveName

curl.exe --fail --location --retry 3 --retry-all-errors --output $archivePath $downloadUrl

$actualHash = (Get-FileHash -Algorithm SHA256 $archivePath).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash.ToLowerInvariant()) {
    throw "ZzXsrv archive checksum mismatch: $actualHash"
}

$extractDir = Join-Path $workDir "zzxsrv-$version"
if (Test-Path $extractDir) {
    Remove-Item -Recurse -Force $extractDir
}
Expand-Archive -Path $archivePath -DestinationPath $extractDir

# The archive carries one top-level directory; package_native.py needs the tree
# whose root holds vcxsrv.exe next to fonts/ and the rest of the runtime.
$dist = Get-ChildItem -Path $extractDir -Directory | Select-Object -First 1
if (-not $dist) {
    throw "ZzXsrv archive has no top-level directory: $archivePath"
}
if (-not (Test-Path (Join-Path $dist.FullName 'vcxsrv.exe'))) {
    throw "ZzXsrv archive is missing vcxsrv.exe: $($dist.FullName)"
}

Write-Output "ZzXsrv distribution at $($dist.FullName)"

if ($env:GITHUB_ENV) {
    "ZZCLAWTERM_VCXSRV_DIST=$($dist.FullName)" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
} else {
    $env:ZZCLAWTERM_VCXSRV_DIST = $dist.FullName
    Write-Output "ZZCLAWTERM_VCXSRV_DIST set for this session (not exported outside CI)"
}

Write-Output '::endgroup::'
