# Installs a checksum-verified portable NSIS and puts makensis.exe on PATH.
#
# Version and digest are pinned together; bump both or the download is rejected.
$ErrorActionPreference = 'Stop'

$version = if ($env:NSIS_VERSION) { $env:NSIS_VERSION } else { '3.12' }
$expectedHash = if ($env:NSIS_ARCHIVE_SHA256) {
    $env:NSIS_ARCHIVE_SHA256
} else {
    '56581f90db321581c5381193d796fffcf2d24b2f8fed2160a6c6a3baa67f2c4f'
}

Write-Output "::group::Install NSIS $version"

$archiveName = "nsis-$version.zip"
$archivePath = Join-Path $env:RUNNER_TEMP $archiveName
$downloadUrl = "https://downloads.sourceforge.net/project/nsis/NSIS%203/$version/$archiveName"

curl.exe --fail --location --retry 3 --retry-all-errors --output $archivePath $downloadUrl

$actualHash = (Get-FileHash -Algorithm SHA256 $archivePath).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash.ToLowerInvariant()) {
    throw "NSIS archive checksum mismatch: $actualHash"
}

$extractRoot = Join-Path $env:RUNNER_TEMP "nsis-$version-portable"
Expand-Archive -Path $archivePath -DestinationPath $extractRoot -Force

$nsisBin = Join-Path $extractRoot "nsis-$version"
if (-not (Test-Path (Join-Path $nsisBin 'makensis.exe'))) {
    throw 'makensis.exe is missing from the verified NSIS archive'
}

$nsisBin | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append

Write-Output '::endgroup::'
