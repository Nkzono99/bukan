# Bootstrap only: verify the release, fetch pinned uv, and obtain managed Python.
# Installation itself is shared Python code; no preinstalled Python is required.
[CmdletBinding()]
param([string]$Bundle)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $Bundle) { $Bundle = $PSScriptRoot }
[Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
$OutputEncoding = [Console]::OutputEncoding
$env:PSModulePath = (Join-Path $PSHOME 'Modules') + ';' + $env:PSModulePath

function Get-InstallerPath([string]$Path) {
    # Rust canonical paths can use Win32's extended prefix; .NET Framework 4's
    # Path APIs and Windows PowerShell 5.1 cmdlets require the ordinary form.
    if ($Path.StartsWith('\\?\UNC\')) { $Path = '\\' + $Path.Substring(8) }
    elseif ($Path.StartsWith('\\?\')) { $Path = $Path.Substring(4) }
    return [IO.Path]::GetFullPath($Path)
}

function Assert-OrdinaryPath([string]$Path) {
    $itemPath = Get-InstallerPath $Path
    while ($itemPath) {
        if (Test-Path -LiteralPath $itemPath) {
            if ((Get-Item -Force -LiteralPath $itemPath).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Linked installer paths are not supported: $itemPath"
            }
        }
        $itemPath = [IO.Path]::GetDirectoryName($itemPath)
    }
}

try {
    if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -eq 'ARM64' -or $env:PROCESSOR_ARCHITEW6432 -eq 'ARM64') {
        throw 'This installer requires x64 Windows.'
    }
    $Bundle = [IO.Path]::GetFullPath($Bundle)
    Assert-OrdinaryPath $Bundle
    $metadata = Get-Content -LiteralPath (Join-Path $Bundle 'bundle.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($metadata.formatVersion -ne 1 -or $metadata.target -ne 'x86_64-pc-windows-msvc' -or $metadata.binary -ne 'bin/bukan.exe') {
        throw 'Use the Bukan Windows x64 release bundle.'
    }
    $required = @('bin/bukan.exe', 'windows-dependencies.json', 'install_windows.py', 'install_local.py')
    foreach ($name in $required) {
        if ($name -notin $metadata.files.PSObject.Properties.Name) { throw "Incomplete bundle: $name" }
    }
    foreach ($file in $metadata.files.PSObject.Properties) {
        if ($file.Name -match '(^/|\\|:|(^|/)\.\.?(/|$)|//)' -or -not $file.Name) { throw "Invalid bundle path: $($file.Name)" }
        $source = Join-Path $Bundle $file.Name
        Assert-OrdinaryPath $source
        if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $file.Value) { throw "Bundle checksum mismatch: $($file.Name)" }
    }
    $binary = Join-Path $Bundle 'bin/bukan.exe'
    $pathsText = & $binary paths --json
    if ($LASTEXITCODE -ne 0) { throw 'Could not resolve Bukan storage paths.' }
    $paths = $pathsText | ConvertFrom-Json
    $paths.cacheDir = Get-InstallerPath $paths.cacheDir
    $paths.dataDir = Get-InstallerPath $paths.dataDir
    $assets = Get-Content -LiteralPath (Join-Path $Bundle 'windows-dependencies.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    # uv is first downloaded into a disposable bootstrap folder. The Python
    # installer validates/copies it into the private dependency store afterwards.
    $bootstrapParent = Join-Path $paths.cacheDir 'installer'
    Assert-OrdinaryPath $bootstrapParent
    [IO.Directory]::CreateDirectory($bootstrapParent) | Out-Null
    $temporary = Join-Path $bootstrapParent ([Guid]::NewGuid().ToString('N'))
    [IO.Directory]::CreateDirectory($temporary) | Out-Null
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $archive = Join-Path $temporary 'uv.zip'
    Write-Host 'Downloading the pinned Bukan dependency manager...'
    Invoke-WebRequest -UseBasicParsing -Uri $assets.uv.url -OutFile $archive
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $assets.uv.sha256) { throw 'uv checksum mismatch.' }
    Expand-Archive -LiteralPath $archive -DestinationPath (Join-Path $temporary 'uv')
    $uv = Join-Path $temporary 'uv/uv.exe'
    # Share Python/cache with the normal research runtime to avoid two downloads.
    $env:UV_PYTHON_INSTALL_DIR = Join-Path $paths.cacheDir 'research-runtime/python'
    $env:UV_CACHE_DIR = Join-Path $paths.cacheDir 'research-runtime/cache'
    Assert-OrdinaryPath $env:UV_PYTHON_INSTALL_DIR
    Assert-OrdinaryPath $env:UV_CACHE_DIR
    & $uv --no-config python install 3.12 --no-bin
    if ($LASTEXITCODE -ne 0) { throw 'Managed Python installation failed. Rerun install.cmd to retry.' }
    $python = & $uv --no-config python find --managed-python 3.12
    if ($LASTEXITCODE -ne 0) { throw 'Managed Python was not found.' }
    & $python -I -B -X utf8 (Join-Path $Bundle 'install_windows.py') --bundle $Bundle --uv-archive $archive
    if ($LASTEXITCODE -ne 0) { throw 'Bukan installation failed. Rerun install.cmd after fixing the reported problem.' }
} catch {
    Write-Host $_.ScriptStackTrace
    Write-Error -ErrorAction Continue $_
    exit 1
} finally {
    if (Get-Variable temporary -ErrorAction SilentlyContinue) {
        # Delete only this invocation's generated directory under the checked cache.
        $resolved = [IO.Path]::GetFullPath($temporary)
        $expected = [IO.Path]::GetFullPath($bootstrapParent).TrimEnd('\') + '\'
        if ($resolved.StartsWith($expected, [StringComparison]::OrdinalIgnoreCase)) {
            Assert-OrdinaryPath $resolved
            Remove-Item -LiteralPath $resolved -Recurse -Force
        }
    }
}
