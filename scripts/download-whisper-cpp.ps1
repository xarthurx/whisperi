# Download whisper.cpp CLI binary for Tauri sidecar
# Usage: pwsh scripts/download-whisper-cpp.ps1 [-Version v1.8.3]

param(
    [string]$Version = "v1.8.3"
)

$ErrorActionPreference = "Stop"

$repo = "ggml-org/whisper.cpp"
$assetName = "whisper-bin-x64.zip"
$downloadUrl = "https://github.com/$repo/releases/download/$Version/$assetName"

$binariesDir = Join-Path $PSScriptRoot ".." "src-tauri" "binaries"
$targetBinary = Join-Path $binariesDir "whisper-cpp-x86_64-pc-windows-msvc.exe"

# Check if binary already exists
if (Test-Path $targetBinary) {
    Write-Host "whisper-cpp sidecar already exists at $targetBinary"
    Write-Host "Delete it first if you want to re-download."
    exit 0
}

# Ensure binaries directory exists
if (-not (Test-Path $binariesDir)) {
    New-Item -ItemType Directory -Path $binariesDir -Force | Out-Null
}

$tempZip = Join-Path $env:TEMP "whisper-bin-x64.zip"
$tempExtract = Join-Path $env:TEMP "whisper-bin-x64"

try {
    Write-Host "Downloading whisper.cpp $Version ($assetName)..."
    Invoke-WebRequest -Uri $downloadUrl -OutFile $tempZip -UseBasicParsing

    Write-Host "Extracting..."
    if (Test-Path $tempExtract) {
        Remove-Item $tempExtract -Recurse -Force
    }
    Expand-Archive -Path $tempZip -DestinationPath $tempExtract -Force

    # Find the main binary (whisper-cli.exe or main.exe)
    $sourceBinary = $null
    $candidates = @("whisper-cli.exe", "main.exe", "whisper.exe")
    foreach ($candidate in $candidates) {
        $found = Get-ChildItem -Path $tempExtract -Filter $candidate -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($found) {
            $sourceBinary = $found.FullName
            Write-Host "Found binary: $candidate"
            break
        }
    }

    if (-not $sourceBinary) {
        # List what we found for debugging
        Write-Host "Available files in archive:"
        Get-ChildItem -Path $tempExtract -Recurse | ForEach-Object { Write-Host "  $($_.FullName)" }
        throw "Could not find whisper CLI binary in the downloaded archive"
    }

    Copy-Item $sourceBinary $targetBinary
    Write-Host "Installed whisper-cpp sidecar to: $targetBinary"

    # Also copy any DLLs that the binary might need
    $dllFiles = Get-ChildItem -Path (Split-Path $sourceBinary) -Filter "*.dll" -ErrorAction SilentlyContinue
    foreach ($dll in $dllFiles) {
        $destDll = Join-Path $binariesDir $dll.Name
        Copy-Item $dll.FullName $destDll
        Write-Host "Copied dependency: $($dll.Name)"
    }
}
finally {
    # Clean up temp files
    if (Test-Path $tempZip) { Remove-Item $tempZip -Force }
    if (Test-Path $tempExtract) { Remove-Item $tempExtract -Recurse -Force }
}

Write-Host "Done! whisper-cpp sidecar is ready."
