[CmdletBinding()]
param([switch]$NoBundle)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$appDir = Split-Path $PSScriptRoot -Parent
$repoDir = Split-Path $appDir -Parent
$savedFlags = $env:CARGO_ENCODED_RUSTFLAGS
$savedTarget = $env:CARGO_TARGET_DIR
$userDir = [Environment]::GetFolderPath('UserProfile')
$mappings = @(
    @{ Path = $userDir; Replacement = '/build/home' },
    @{ Path = $repoDir; Replacement = '/build/project' },
    @{ Path = $env:CARGO_HOME; Replacement = '/build/cargo' },
    @{ Path = $env:RUSTUP_HOME; Replacement = '/build/rustup' },
    @{ Path = $env:TEMP; Replacement = '/build/tmp' }
)

try {
    $flags = @()
    if ($savedFlags) {
        $flags += $savedFlags -split [char]31
    } elseif ($env:RUSTFLAGS) {
        $flags += $env:RUSTFLAGS -split '\s+' | Where-Object { $_ }
    }
    foreach ($mapping in $mappings) {
        if ($mapping.Path) {
            $flags += '--remap-path-prefix=' + $mapping.Path + '=' + $mapping.Replacement
        }
    }
    $flags += @('-C', 'strip=debuginfo', '-C', 'link-arg=/PDBALTPATH:nacl.pdb')
    $env:CARGO_ENCODED_RUSTFLAGS = $flags -join [char]31
    if (-not $env:CARGO_TARGET_DIR) {
        $env:CARGO_TARGET_DIR = Join-Path $appDir 'target/privacy-build'
    }
    if (-not [IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        $env:CARGO_TARGET_DIR = Join-Path $appDir $env:CARGO_TARGET_DIR
    }

    Push-Location $appDir
    try {
        & npm.cmd run tauri -- build --no-bundle
        if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }

        $releaseDir = Join-Path $env:CARGO_TARGET_DIR 'release'
        $exe = Join-Path $releaseDir 'nacl.exe'
        $bytes = [IO.File]::ReadAllBytes($exe)
        $texts = @([Text.Encoding]::UTF8.GetString($bytes), [Text.Encoding]::Unicode.GetString($bytes))
        foreach ($text in $texts) {
            foreach ($mapping in $mappings) {
                if (-not $mapping.Path) { continue }
                foreach ($prefix in @($mapping.Path, $mapping.Path.Replace('\', '/'))) {
                    if ($text.IndexOf($prefix, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                        throw 'Release executable contains a local build path; publication blocked.'
                    }
                }
            }
            if ($text -match '(?i)[a-z]:[\\/]Users[\\/](?!Public[\\/]|Default[\\/])[^\\/\x00\r\n]{1,64}[\\/]') {
                throw 'Release executable contains a user-directory path; publication blocked.'
            }
        }

        $version = (Get-Content -LiteralPath (Join-Path $appDir 'package.json') -Raw | ConvertFrom-Json).version
        $artifacts = Join-Path $appDir ('target/privacy-artifacts/' + $version)
        New-Item -ItemType Directory -Force -Path $artifacts | Out-Null
        Copy-Item -LiteralPath $exe -Destination (Join-Path $artifacts ('NaCL_' + $version + '_windows-x64-portable.exe'))

        if (-not $NoBundle) {
            & npm.cmd run tauri -- bundle --bundles msi,nsis --ci
            if ($LASTEXITCODE -ne 0) { throw 'Release packaging failed.' }
            Copy-Item -LiteralPath (Join-Path $releaseDir ('bundle/nsis/NaCL_' + $version + '_x64-setup.exe')) -Destination (Join-Path $artifacts ('NaCL_' + $version + '_windows-x64-setup.exe'))
            Copy-Item -LiteralPath (Join-Path $releaseDir ('bundle/msi/NaCL_' + $version + '_x64_en-US.msi')) -Destination (Join-Path $artifacts ('NaCL_' + $version + '_windows-x64.msi'))
        }
        Write-Host 'Release executable privacy check passed.'
        Get-ChildItem -LiteralPath $artifacts -File | Select-Object Name, Length
    } finally {
        Pop-Location
    }
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $savedFlags
    $env:CARGO_TARGET_DIR = $savedTarget
}
