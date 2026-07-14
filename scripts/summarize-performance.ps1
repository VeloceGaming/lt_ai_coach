param(
    [string]$Path = "$env:LOCALAPPDATA\com.lttools.lt-ai-coach\logs\performance.jsonl",
    [int]$Tail = 0
)

$ErrorActionPreference = "Stop"
if (-not (Test-Path -LiteralPath $Path)) {
    throw "Performance log not found: $Path"
}

$lines = if ($Tail -gt 0) { Get-Content -LiteralPath $Path -Tail $Tail } else { Get-Content -LiteralPath $Path }
$events = @($lines | ForEach-Object {
    try { $_ | ConvertFrom-Json } catch { $null }
} | Where-Object { $null -ne $_.durationUs })

if ($events.Count -eq 0) {
    Write-Host "No duration events found in $Path"
    exit 0
}

$summary = foreach ($group in ($events | Group-Object component, action)) {
    $values = @($group.Group | ForEach-Object { [double]$_.durationUs / 1000.0 } | Sort-Object)
    $count = $values.Count
    $average = ($values | Measure-Object -Average).Average
    $p50 = $values[[Math]::Min($count - 1, [Math]::Floor(($count - 1) * 0.50))]
    $p95 = $values[[Math]::Min($count - 1, [Math]::Ceiling(($count - 1) * 0.95))]
    [PSCustomObject]@{
        Component = $group.Group[0].component
        Action = $group.Group[0].action
        Count = $count
        AvgMs = [Math]::Round($average, 2)
        P50Ms = [Math]::Round($p50, 2)
        P95Ms = [Math]::Round($p95, 2)
        MaxMs = [Math]::Round($values[-1], 2)
    }
}

$summary | Sort-Object Component, Action | Format-Table -AutoSize

Write-Host "`nSlowest events"
$events |
    Sort-Object { [double]$_.durationUs } -Descending |
    Select-Object -First 15 `
        @{Name="Time";Expression={[DateTimeOffset]::FromUnixTimeMilliseconds([long]$_.timestampUnixMs).LocalDateTime}},
        component,
        action,
        @{Name="DurationMs";Expression={[Math]::Round([double]$_.durationUs / 1000.0, 2)}},
        details |
    Format-Table -AutoSize
