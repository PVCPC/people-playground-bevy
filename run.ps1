$ErrorActionPreference = 'Stop'
if (!$env:WGPU_BACKEND) { $env:WGPU_BACKEND = 'dx12' }
Push-Location $PSScriptRoot
try {
    & cargo run -- @args
    $gameExitCode = $LASTEXITCODE
} finally {
    Pop-Location
}
exit $gameExitCode
