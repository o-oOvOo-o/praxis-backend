param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$FlywheelArgs
)

$ErrorActionPreference = "Stop"

function Resolve-Cunning3DRoot {
    param([string]$Candidate)

    if ([string]::IsNullOrWhiteSpace($Candidate)) {
        return $null
    }
    $resolved = Resolve-Path -LiteralPath $Candidate -ErrorAction SilentlyContinue
    if ($null -eq $resolved) {
        return $null
    }
    $path = Get-Item -LiteralPath $resolved.Path
    if (-not $path.PSIsContainer) {
        $path = $path.Directory
    }
    while ($null -ne $path) {
        if (Test-Path -LiteralPath (Join-Path $path.FullName "crates/cunning_core/Cargo.toml") -PathType Leaf) {
            return $path.FullName
        }
        $path = $path.Parent
    }
    return $null
}

function Find-Cunning3DRoot {
    $candidates = @(
        $env:CUNNING3D_ROOT,
        (Get-Location).Path,
        $PSCommandPath
    )
    foreach ($candidate in $candidates) {
        $root = Resolve-Cunning3DRoot $candidate
        if ($null -ne $root) {
            return $root
        }
    }
    throw "Could not find the Cunning3D repository. Run from the repository or set CUNNING3D_ROOT."
}

$pluginRoot = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
$runtime = Join-Path $pluginRoot "runtime/c3d_devflywheeltool"
$runner = Join-Path $runtime "run.ps1"
if (-not (Test-Path -LiteralPath $runner -PathType Leaf)) {
    throw "The plugin-owned Gaea flywheel runtime is missing: $runner"
}

$env:CUNNING3D_ROOT = Find-Cunning3DRoot
$env:C3D_DEVFLYWHEEL_DIR = $runtime
$env:C3D_DEVFLYWHEEL_ARTIFACT_ROOT = Join-Path $env:CUNNING3D_ROOT ".local/gaea-flywheel/artifacts"
$env:C3D_GAEA_HARNESS_EXE = Join-Path $env:CUNNING3D_ROOT ".local/gaea/harness/bin/Debug/net8.0-windows/GaeaReverseHarness.exe"
if ($null -eq $FlywheelArgs -or $FlywheelArgs.Count -eq 0) {
    $FlywheelArgs = @("toolbox", "--json")
}

& $runner -- @FlywheelArgs
exit $LASTEXITCODE
