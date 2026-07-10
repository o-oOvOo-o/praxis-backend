$ErrorActionPreference = "Stop"

function New-Panel {
    param(
        [string]$Title,
        [string]$Subtitle,
        [array]$Rows
    )
    [ordered]@{
        title = $Title
        subtitle = $Subtitle
        filters = @(
            [ordered]@{
                label = "State"
                options = @(
                    [ordered]@{ id = "all"; label = "All" },
                    [ordered]@{ id = "active"; label = "Active" },
                    [ordered]@{ id = "blocked"; label = "Blocked" },
                    [ordered]@{ id = "open"; label = "Open" },
                    [ordered]@{ id = "ready"; label = "Ready" }
                )
            }
        )
        rows = $Rows
    }
}

function New-ErrorPanel {
    param([string]$Message)
    New-Panel -Title "Gaea Flywheel" -Subtitle "Ledger unavailable" -Rows @(
        [ordered]@{
            name = "Error"
            description = $Message
            category = "Status"
            status = "Error"
            progressPercent = 0
            filter = "blocked"
            details = @()
        }
    )
}

function Test-RepoRoot {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $false
    }
    $ledger = Join-Path $Path "tools/c3d_devflywheeltool/ledger/gaea_flywheel_graph.json"
    return Test-Path -LiteralPath $ledger -PathType Leaf
}

function Find-RepoRoot {
    $scriptDir = Split-Path -Parent $PSCommandPath
    $candidates = @(
        $env:CUNNING3D_ROOT,
        $env:C3D_REPO_ROOT,
        (Join-Path $scriptDir "../../../../../.."),
        (Join-Path $scriptDir "../../../../../../.."),
        (Join-Path (Get-Location).Path "."),
        "D:/ghost1.0/Cunning3D_1.0"
    )

    foreach ($candidate in $candidates) {
        if ([string]::IsNullOrWhiteSpace($candidate)) {
            continue
        }
        try {
            $resolved = (Resolve-Path -LiteralPath $candidate -ErrorAction Stop).Path
        } catch {
            continue
        }
        if (Test-RepoRoot $resolved) {
            return $resolved
        }
    }

    foreach ($start in @($scriptDir, (Get-Location).Path)) {
        $current = Get-Item -LiteralPath $start -ErrorAction SilentlyContinue
        while ($null -ne $current) {
            if (Test-RepoRoot $current.FullName) {
                return $current.FullName
            }
            $current = $current.Parent
        }
    }

    return $null
}

function Add-Node {
    param(
        [hashtable]$Nodes,
        [string]$Name
    )
    if ([string]::IsNullOrWhiteSpace($Name)) {
        return $null
    }
    if (-not $Nodes.ContainsKey($Name)) {
        $Nodes[$Name] = [ordered]@{
            name = $Name
            contracts = New-Object System.Collections.ArrayList
            matrix = New-Object System.Collections.ArrayList
        }
    }
    return $Nodes[$Name]
}

function Add-UniqueDetail {
    param(
        [System.Collections.Generic.List[string]]$Details,
        [string]$Value
    )
    if (-not [string]::IsNullOrWhiteSpace($Value) -and -not $Details.Contains($Value)) {
        [void]$Details.Add($Value)
    }
}

function Get-Double {
    param($Value)
    if ($null -eq $Value) {
        return $null
    }
    try {
        return [double]$Value
    } catch {
        return $null
    }
}

function Get-RowScore {
    param($Row)
    $caseCount = Get-Double $Row.case_count
    $exactCount = Get-Double $Row.exact_count
    $failedCount = Get-Double $Row.failed_count
    $exactRatio = 0.0
    if ($caseCount -gt 0 -and $null -ne $exactCount) {
        $exactRatio = [Math]::Min(1.0, [Math]::Max(0.0, $exactCount / $caseCount))
    } elseif ($failedCount -eq 0) {
        $exactRatio = 1.0
    }

    $statusText = "$($Row.ledger_status) $($Row.promotion_status)"
    $score = 20.0 + ($exactRatio * 50.0)
    if ($failedCount -eq 0) {
        $score += 10.0
    }
    if ($Row.speed_gate_passed -eq $true) {
        $score += 10.0
    }
    if ($statusText -match "passed_exact|audited|closed") {
        $score += 10.0
    }
    if ($Row.ledger_status -match "^open" -and $statusText -notmatch "passed_exact") {
        $score = [Math]::Min($score, 78.0)
    }
    if ($failedCount -gt 0 -or -not [string]::IsNullOrWhiteSpace($Row.blocker_class)) {
        $score = [Math]::Min(55.0, $exactRatio * 55.0)
    }
    return [Math]::Min(100.0, [Math]::Max(0.0, $score))
}

try {
    $repoRoot = Find-RepoRoot
    if ($null -eq $repoRoot) {
        New-ErrorPanel -Message "Could not find Cunning3D_1.0. Set CUNNING3D_ROOT to the repository root." | ConvertTo-Json -Depth 8 -Compress
        exit 0
    }

    $ledgerDir = Join-Path $repoRoot "tools/c3d_devflywheeltool/ledger"
    $matrixPath = Join-Path $ledgerDir "gaea_node_performance_acceptance_matrix.json"
    $graphPath = Join-Path $ledgerDir "gaea_flywheel_graph.json"

    if (-not (Test-Path -LiteralPath $matrixPath -PathType Leaf) -or -not (Test-Path -LiteralPath $graphPath -PathType Leaf)) {
        New-ErrorPanel -Message "Missing Gaea flywheel ledger files under $ledgerDir." | ConvertTo-Json -Depth 8 -Compress
        exit 0
    }

    $matrix = Get-Content -LiteralPath $matrixPath -Raw | ConvertFrom-Json
    $graph = Get-Content -LiteralPath $graphPath -Raw | ConvertFrom-Json
    $nodes = @{}

    foreach ($contract in @($graph.contracts)) {
        foreach ($nodeName in @($contract.owner_nodes + $contract.unlocks)) {
            $node = Add-Node -Nodes $nodes -Name $nodeName
            if ($null -ne $node) {
                [void]$node.contracts.Add($contract)
            }
        }
    }

    foreach ($matrixRow in @($matrix.rows)) {
        $node = Add-Node -Nodes $nodes -Name $matrixRow.node
        if ($null -ne $node) {
            [void]$node.matrix.Add($matrixRow)
        }
    }

    $panelRows = New-Object System.Collections.ArrayList
    foreach ($entry in $nodes.GetEnumerator()) {
        $node = $entry.Value
        $matrixRows = @($node.matrix)
        $contracts = @($node.contracts)
        $details = New-Object 'System.Collections.Generic.List[string]'

        $caseCount = 0
        $exactCount = 0
        $failedCount = 0
        $blockedRows = 0
        $rowScoreTotal = 0.0
        $speedups = New-Object 'System.Collections.Generic.List[double]'
        $nextCommand = $null

        foreach ($row in $matrixRows) {
            $rowCases = Get-Double $row.case_count
            $rowExact = Get-Double $row.exact_count
            $rowFailed = Get-Double $row.failed_count
            if ($null -ne $rowCases) { $caseCount += [int]$rowCases }
            if ($null -ne $rowExact) { $exactCount += [int]$rowExact }
            if ($null -ne $rowFailed) { $failedCount += [int]$rowFailed }
            if (($null -ne $rowFailed -and $rowFailed -gt 0) -or -not [string]::IsNullOrWhiteSpace($row.blocker_class) -or "$($row.promotion_status)" -match "blocked") {
                $blockedRows += 1
            }
            $rowScoreTotal += Get-RowScore $row
            foreach ($speed in @($row.actual_speedup, $row.actual_fused_speedup)) {
                $speedValue = Get-Double $speed
                if ($null -ne $speedValue) {
                    [void]$speedups.Add($speedValue)
                }
            }
            if ($null -eq $nextCommand -and -not [string]::IsNullOrWhiteSpace($row.next_command)) {
                $nextCommand = $row.next_command
            }
        }

        $contractCount = $contracts.Count
        $closedContracts = @($contracts | Where-Object { "$($_.status)" -match "closed|audited|focused" }).Count
        $openContracts = @($contracts | Where-Object { "$($_.status)" -match "open" }).Count

        if ($caseCount -gt 0) {
            Add-UniqueDetail -Details $details -Value ("cases {0}/{1}" -f $exactCount, $caseCount)
        }
        if ($failedCount -gt 0) {
            Add-UniqueDetail -Details $details -Value ("failed {0}" -f $failedCount)
        }
        if ($matrixRows.Count -gt 0) {
            Add-UniqueDetail -Details $details -Value ("matrix scopes {0}" -f $matrixRows.Count)
        }
        if ($contractCount -gt 0) {
            Add-UniqueDetail -Details $details -Value ("contracts {0}/{1} closed" -f $closedContracts, $contractCount)
        }
        if ($speedups.Count -gt 0) {
            $maxSpeedup = ($speedups | Measure-Object -Maximum).Maximum
            Add-UniqueDetail -Details $details -Value ("max speedup {0:n1}x" -f $maxSpeedup)
        }
        if ($null -eq $nextCommand) {
            $nextFromContract = @($contracts | Where-Object { $_.next_commands -and $_.next_commands.Count -gt 0 } | Select-Object -First 1)
            if ($nextFromContract.Count -gt 0) {
                $nextCommand = @($nextFromContract[0].next_commands)[0]
            }
        }
        if (-not [string]::IsNullOrWhiteSpace($nextCommand)) {
            $compactNext = $nextCommand -replace "^\.\\tools\\c3d_devflywheeltool\\run\.ps1 -- ", ""
            if ($compactNext.Length -gt 96) {
                $compactNext = $compactNext.Substring(0, 93) + "..."
            }
            Add-UniqueDetail -Details $details -Value ("next $compactNext")
        }

        $progress = 0.0
        if ($matrixRows.Count -gt 0) {
            $progress = $rowScoreTotal / $matrixRows.Count
        } elseif ($contractCount -gt 0) {
            $progress = 20.0 + (60.0 * ($closedContracts / [Math]::Max(1, $contractCount)))
            if ($openContracts -gt 0) {
                $progress = [Math]::Min($progress, 70.0)
            }
        } else {
            $progress = 5.0
        }
        $progress = [Math]::Round([Math]::Min(100.0, [Math]::Max(0.0, $progress)), 1)

        if ($blockedRows -gt 0 -or $failedCount -gt 0) {
            $filter = "blocked"
            $status = "Blocked"
        } elseif ($matrixRows.Count -gt 0 -and $failedCount -eq 0 -and $progress -ge 90.0) {
            $filter = "ready"
            $status = "Ready"
        } elseif ($matrixRows.Count -gt 0 -or ($closedContracts -gt 0 -and $openContracts -gt 0)) {
            $filter = "active"
            $status = "Active"
        } elseif ($contractCount -gt 0 -and $closedContracts -eq $contractCount) {
            $filter = "ready"
            $status = "Ready"
        } else {
            $filter = "open"
            $status = "Open"
        }

        $category = if ($matrixRows.Count -gt 0) { "Acceptance" } elseif ($contractCount -gt 0) { "Graph" } else { "Ledger" }
        $description = if ($matrixRows.Count -gt 0) {
            "Performance acceptance matrix and flywheel graph evidence"
        } else {
            "Flywheel graph contract evidence"
        }

        [void]$panelRows.Add([ordered]@{
            name = $node.name
            description = $description
            category = $category
            status = $status
            progressPercent = $progress
            filter = $filter
            details = @($details)
        })
    }

    $statusRank = @{ blocked = 0; active = 1; open = 2; ready = 3 }
    $sortedRows = @($panelRows | Sort-Object @{ Expression = { $statusRank[$_.filter] } }, @{ Expression = { $_.name } })
    $readyCount = @($sortedRows | Where-Object { $_.filter -eq "ready" }).Count
    $activeCount = @($sortedRows | Where-Object { $_.filter -eq "active" }).Count
    $blockedCount = @($sortedRows | Where-Object { $_.filter -eq "blocked" }).Count
    $openCount = @($sortedRows | Where-Object { $_.filter -eq "open" }).Count
    $subtitle = "Nodes $($sortedRows.Count) | ready $readyCount | active $activeCount | blocked $blockedCount | open $openCount"

    New-Panel -Title "Gaea Flywheel" -Subtitle $subtitle -Rows $sortedRows | ConvertTo-Json -Depth 12 -Compress
} catch {
    New-ErrorPanel -Message $_.Exception.Message | ConvertTo-Json -Depth 8 -Compress
    exit 0
}
