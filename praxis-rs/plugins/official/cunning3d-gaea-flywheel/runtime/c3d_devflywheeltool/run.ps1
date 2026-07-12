[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]] $Args
)

$ErrorActionPreference = 'Stop'

$targetDir = if ($env:C3D_DEVFLYWHEELTOOL_TARGET_DIR) {
    $env:C3D_DEVFLYWHEELTOOL_TARGET_DIR
} else {
    'F:\cargo-target2\Praxis-cunning3d-gaea-flywheel'
}

$env:CARGO_TARGET_DIR = $targetDir
$manifest = Join-Path $PSScriptRoot 'Cargo.toml'
$cargoArgs = @('run', '--manifest-path', $manifest, '--') + $Args

& cargo @cargoArgs
exit $LASTEXITCODE
