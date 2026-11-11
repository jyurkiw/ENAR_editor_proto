# Builds the standalone Enigma Narrative Editor.
#
#   .\build-release.ps1            the .exe and an installer
#   .\build-release.ps1 -NoBundle  just the .exe (faster)
#
# Output:
#   release\Enigma Narrative Editor.exe          run it from anywhere; double-click it
#   release\Enigma Narrative Editor_<v>_x64-setup.exe   the installer (Start menu entry, uninstaller)
#
# Needs: Rust, the wasm32-unknown-unknown target, trunk 0.21 and tauri-cli 2 (see README).
# The app needs at run time: WebView2 (part of Windows 11) and uv (for validation).
param([switch]$NoBundle)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
# trunk reads NO_COLOR as true/false and stops on NO_COLOR=1, which some terminals set.
Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue

$cargoArgs = @('tauri', 'build')
if ($NoBundle) { $cargoArgs += '--no-bundle' }
& cargo @cargoArgs
if ($LASTEXITCODE -ne 0) { throw "cargo tauri build failed ($LASTEXITCODE)" }

New-Item -ItemType Directory -Force release | Out-Null
Copy-Item target\release\enigma-editor.exe 'release\Enigma Narrative Editor.exe' -Force
if (-not $NoBundle) {
    Get-ChildItem target\release\bundle\nsis\*-setup.exe | Copy-Item -Destination release -Force
}
Write-Host ''
Write-Host 'Built:'
Get-ChildItem release | ForEach-Object { Write-Host "  $($_.FullName)" }
