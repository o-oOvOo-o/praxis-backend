param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$FlywheelArgs
)

$ErrorActionPreference = "Stop"

function Resolve-Ghost1Root {
    param([string]$Candidate)

    if ([string]::IsNullOrWhiteSpace($Candidate)) {
        return $null
    }
    $resolved = Resolve-Path -LiteralPath $Candidate -ErrorAction SilentlyContinue
    if ($null -eq $resolved) {
        return $null
    }
    $path = $resolved.Path
    if (Test-Path -LiteralPath (Join-Path $path "Cunning3D_1.0/crates/cunning_core/Cargo.toml") -PathType Leaf) {
        return $path
    }
    if (Test-Path -LiteralPath (Join-Path $path "crates/cunning_core/Cargo.toml") -PathType Leaf) {
        return (Split-Path -Parent $path)
    }
    return $null
}

function Find-Ghost1Root {
    $candidates = @(
        $env:GHOST1_ROOT,
        $env:CUNNING3D_ROOT,
        $env:C3D_REPO_ROOT,
        (Get-Location).Path,
        "D:/ghost1.0"
    )
    foreach ($candidate in $candidates) {
        $root = Resolve-Ghost1Root $candidate
        if ($null -ne $root) {
            return $root
        }
    }
    throw "Could not find the Cunning3D workspace. Set GHOST1_ROOT or CUNNING3D_ROOT."
}

$pluginRoot = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
$runtime = Join-Path $pluginRoot "runtime/c3d_devflywheeltool"
$runner = Join-Path $runtime "run.ps1"
if (-not (Test-Path -LiteralPath $runner -PathType Leaf)) {
    throw "The plugin-owned Gaea flywheel runtime is missing: $runner"
}

$env:GHOST1_ROOT = Find-Ghost1Root
$env:C3D_DEVFLYWHEEL_DIR = $runtime
if ($null -eq $FlywheelArgs -or $FlywheelArgs.Count -eq 0) {
    $FlywheelArgs = @("toolbox", "--json")
}

& $runner -- @FlywheelArgs
exit $LASTEXITCODE
