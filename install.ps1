# Installs the `karbon` CLI on Windows from the prebuilt binary of a GitHub release.
#
#   irm https://raw.githubusercontent.com/larevuegeek/karbon/main/install.ps1 | iex
#
# Environment:
#   KARBON_VERSION      release to install, e.g. 0.5.1 (default: the latest)
#   KARBON_INSTALL_DIR  where to put the binary (default: %USERPROFILE%\.cargo\bin if it
#                       exists, else %LOCALAPPDATA%\karbon\bin)
#   KARBON_DOWNLOAD_URL base URL holding the archives, for a mirror (default: the GitHub
#                       release)
#
# Karbon projects are Rust projects: you still need Rust (https://rustup.rs) to run them.
# This script only saves compiling the CLI itself (`cargo install karbon-cli`).

$ErrorActionPreference = 'Stop'

$repo = 'larevuegeek/karbon'
$target = 'x86_64-pc-windows-msvc'
$archive = "karbon-$target.zip"

if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    Write-Host "No prebuilt binary for $($env:PROCESSOR_ARCHITECTURE). Build from source: cargo install karbon-cli"
    exit 1
}

if ($env:KARBON_DOWNLOAD_URL) {
    $base = $env:KARBON_DOWNLOAD_URL.TrimEnd('/')
} elseif ($env:KARBON_VERSION) {
    $base = "https://github.com/$repo/releases/download/v$($env:KARBON_VERSION.TrimStart('v'))"
} else {
    $base = "https://github.com/$repo/releases/latest/download"
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) "karbon-install-$PID"
New-Item -ItemType Directory -Force $tmp | Out-Null

try {
    Write-Host "Downloading $base/$archive"
    Invoke-WebRequest "$base/$archive" -OutFile (Join-Path $tmp $archive) -UseBasicParsing

    # Verify the checksum published next to the archive.
    $sumFile = Join-Path $tmp "$archive.sha256"
    $published = $true
    try {
        Invoke-WebRequest "$base/$archive.sha256" -OutFile $sumFile -UseBasicParsing
    } catch {
        $published = $false
    }
    if ($published) {
        $expected = ((Get-Content $sumFile -Raw).Trim() -split '\s+')[0].ToLower()
        $actual = (Get-FileHash (Join-Path $tmp $archive) -Algorithm SHA256).Hash.ToLower()
        if ($actual -ne $expected) {
            throw "Checksum mismatch for $archive (expected $expected, got $actual)"
        }
    } else {
        Write-Host "warning: no checksum published for $archive - not verified"
    }

    Expand-Archive (Join-Path $tmp $archive) -DestinationPath $tmp -Force

    if ($env:KARBON_INSTALL_DIR) {
        $dir = $env:KARBON_INSTALL_DIR
    } elseif (Test-Path (Join-Path $env:USERPROFILE '.cargo\bin')) {
        $dir = Join-Path $env:USERPROFILE '.cargo\bin'
    } else {
        $dir = Join-Path $env:LOCALAPPDATA 'karbon\bin'
    }
    New-Item -ItemType Directory -Force $dir | Out-Null
    Copy-Item (Join-Path $tmp "karbon-$target\karbon.exe") (Join-Path $dir 'karbon.exe') -Force

    Write-Host ''
    Write-Host "Installed: $(Join-Path $dir 'karbon.exe')"

    if (-not (($env:PATH -split ';') -contains $dir)) {
        Write-Host "Add $dir to your PATH to run ``karbon`` from anywhere."
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Host 'Rust is not installed yet - get it at https://rustup.rs, Karbon projects need it.'
    }

    Write-Host ''
    Write-Host 'Next: karbon new my-app; cd my-app; karbon dev'
} finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
