$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Fail([string]$Message) {
    throw "rai install: $Message"
}

if (-not [Environment]::Is64BitOperatingSystem) {
    Fail 'no prebuilt rai release for 32-bit Windows'
}

$repo = 'Gamma-Software/RosettAI'
$release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest"
$tag = [string]$release.tag_name
if ($tag -cnotmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') {
    Fail "invalid release tag: $tag"
}

$archive = "rai-$tag-x86_64-pc-windows-msvc.zip"
$baseUrl = "https://github.com/$repo/releases/download/$tag"
$workDir = Join-Path ([IO.Path]::GetTempPath()) ("rai-install-" + [guid]::NewGuid().ToString('N'))
$stagedBinary = $null
New-Item -ItemType Directory -Path $workDir | Out-Null

try {
    $checksumsPath = Join-Path $workDir 'SHA256SUMS'
    $archivePath = Join-Path $workDir $archive
    Invoke-WebRequest -Uri "$baseUrl/SHA256SUMS" -OutFile $checksumsPath -UseBasicParsing
    Invoke-WebRequest -Uri "$baseUrl/$archive" -OutFile $archivePath -UseBasicParsing

    $checksumMatches = @(Get-Content $checksumsPath | ForEach-Object {
        if ($_ -match '^([0-9a-fA-F]{64})\s+\*?(.+)$' -and $Matches[2] -ceq $archive) {
            $Matches[1]
        }
    })
    if ($checksumMatches.Count -ne 1) {
        Fail "SHA256SUMS has no unique valid entry for $archive"
    }
    $actual = (Get-FileHash -Path $archivePath -Algorithm SHA256).Hash
    if ($actual -ine $checksumMatches[0]) {
        Fail "SHA-256 mismatch for $archive"
    }

    $unpackDir = Join-Path $workDir 'unpack'
    Expand-Archive -Path $archivePath -DestinationPath $unpackDir
    $binary = Join-Path $unpackDir 'rai.exe'
    if (-not (Test-Path $binary -PathType Leaf)) {
        Fail "release archive contains no rai.exe"
    }

    $installDir = if ($env:RAI_INSTALL_DIR) {
        $env:RAI_INSTALL_DIR
    } else {
        Join-Path $env:LOCALAPPDATA 'Programs\rai'
    }
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    $installDir = (Resolve-Path $installDir).Path
    $destination = Join-Path $installDir 'rai.exe'
    $stagedBinary = Join-Path $installDir ('.rai-install-' + [guid]::NewGuid().ToString('N') + '.exe')
    Copy-Item -Path $binary -Destination $stagedBinary
    try {
        Move-Item -Path $stagedBinary -Destination $destination -Force
    } catch {
        Fail "cannot replace $destination; close any running rai process and retry"
    }
    $stagedBinary = $null

    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $pathEntries = @($userPath -split ';' | Where-Object { $_ })
    if ($pathEntries -notcontains $installDir) {
        $newPath = if ($userPath) { "$userPath;$installDir" } else { $installDir }
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    }
    if (@($env:Path -split ';') -notcontains $installDir) {
        $env:Path += ";$installDir"
    }

    Write-Output "Installed rai $tag at $destination"
    Write-Output 'Open a new terminal, then run: rai init'
} finally {
    if ($stagedBinary -and (Test-Path $stagedBinary)) {
        Remove-Item $stagedBinary -Force
    }
    Remove-Item $workDir -Recurse -Force
}
