# Installs a checksum-verified Inno Setup 7 and puts ISCC.exe on PATH.
#
# Version and digest are pinned together; bump both or the download is rejected.
$ErrorActionPreference = 'Stop'

$version = if ($env:INNO_SETUP_VERSION) { $env:INNO_SETUP_VERSION } else { '7.1.0' }
$expectedHash = if ($env:INNO_SETUP_INSTALLER_SHA256) {
    $env:INNO_SETUP_INSTALLER_SHA256
} else {
    '0362a383ed217d4c4239b5933866dd96d3eb2102737da92f80f6057a4b40df2f'
}

Write-Output "::group::Install Inno Setup $version"

$downloadDir = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { $env:TEMP }
$installerPath = Join-Path $downloadDir "innosetup-$version-x64.exe"
$tag = "is-$($version -replace '\.', '_')"
$downloadUrl = "https://github.com/jrsoftware/issrc/releases/download/$tag/innosetup-$version-x64.exe"

curl.exe --fail --location --retry 3 --retry-all-errors --output $installerPath $downloadUrl

$actualHash = (Get-FileHash -Algorithm SHA256 $installerPath).Hash.ToLowerInvariant()
if ($actualHash -ne $expectedHash.ToLowerInvariant()) {
    throw "Inno Setup installer checksum mismatch: $actualHash"
}

& $installerPath /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /SP- | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup installer exited with code $LASTEXITCODE"
}

$iscc = @(
    (Join-Path $env:ProgramFiles 'Inno Setup 7\ISCC.exe'),
    (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 7\ISCC.exe')
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) {
    throw 'ISCC.exe is missing after the Inno Setup install'
}

if ($env:GITHUB_PATH) {
    Split-Path $iscc | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
} else {
    Write-Output "Inno Setup installed at $iscc (not on PATH outside CI; package_native.py finds it automatically)"
}

Write-Output '::endgroup::'
