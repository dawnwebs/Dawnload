# Measures Dawnload's Slint prototype on Windows: memory and CPU with the window open and idle,
# with 500 demo items updating, and while the list scrolls. Twin of scripts/measure-ui.sh.
#
#   pwsh scripts/measure-ui.ps1
#   pwsh scripts/measure-ui.ps1 -SecondsPer 60
param(
    [int]$SecondsPer = 30,
    [int]$Warmup = 8
)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$Out = "target/measure-ui-windows.md"

cargo build --release -q -p dawnload-ui
if ($LASTEXITCODE -ne 0) { throw "build failed" }
$Bin = (Resolve-Path "target/release/dawnload-app.exe").Path

function Scenario([string]$Name, [hashtable]$Vars) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Bin
    $psi.UseShellExecute = $false
    $psi.RedirectStandardError = $true
    $psi.EnvironmentVariables["DAWNLOAD_UI_STATS"] = "1"
    foreach ($k in $Vars.Keys) { $psi.EnvironmentVariables[$k] = $Vars[$k] }
    $proc = [System.Diagnostics.Process]::Start($psi)
    $stats = New-Object System.Collections.Generic.List[string]
    $reader = $proc.StandardError
    Start-Sleep -Seconds $Warmup
    $proc.Refresh()
    $cpu0 = $proc.TotalProcessorTime.TotalSeconds
    $ws = @(); $private = @()
    for ($i = 0; $i -lt $SecondsPer; $i++) {
        Start-Sleep -Seconds 1
        $proc.Refresh()
        $ws += $proc.WorkingSet64 / 1MB
        $private += $proc.PrivateMemorySize64 / 1MB
        while ($reader.Peek() -ge 0) { $stats.Add($reader.ReadLine()) }
    }
    $proc.Refresh()
    $cpu1 = $proc.TotalProcessorTime.TotalSeconds
    Stop-Process -Id $proc.Id -Force
    $fps = ($stats | Select-String -Pattern 'fps=(\d+)' | ForEach-Object { [int]$_.Matches[0].Groups[1].Value } | Select-Object -Last $SecondsPer | Measure-Object -Average).Average
    $rows = ($stats | Select-String -Pattern 'rows_patched=(\d+)' | ForEach-Object { [int]$_.Matches[0].Groups[1].Value } | Select-Object -Last $SecondsPer | Measure-Object -Average).Average
    $line = "| {0} | {1:N1} | {2:N1} | {3:N1} | {4:N1}% | {5:N0} | {6:N0} |" -f $Name,
        ($ws | Measure-Object -Average).Average, ($ws | Measure-Object -Maximum).Maximum,
        ($private | Measure-Object -Average).Average, (($cpu1 - $cpu0) / $SecondsPer * 100), $fps, $rows
    Write-Output $line
    Add-Content -Path $Out -Value $line
}

$header = @(
    "# Dawnload UI measurement", "",
    "$([System.Environment]::OSVersion.VersionString) $env:PROCESSOR_ARCHITECTURE, $((Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mmZ')), ${SecondsPer}s per scenario after ${Warmup}s warm-up, release build.", "",
    "| Scenario | Working set avg (MB) | Working set peak (MB) | Private (MB) | CPU (one core) | Frames/s | Rows patched/s |",
    "|---|---|---|---|---|---|---|"
)
Set-Content -Path $Out -Value $header
$header | Write-Output
Scenario "Idle, empty list" @{ DAWNLOAD_DEMO_ITEMS = "0" }
Scenario "Idle, 500 items updating" @{ DAWNLOAD_DEMO_ITEMS = "500" }
Scenario "Scrolling, 500 items updating" @{ DAWNLOAD_DEMO_ITEMS = "500"; DAWNLOAD_UI_AUTOSCROLL = "14" }
Scenario "Scrolling, 500 items, eased rows" @{ DAWNLOAD_DEMO_ITEMS = "500"; DAWNLOAD_UI_AUTOSCROLL = "14"; DAWNLOAD_UI_EASE_ROWS = "1" }
Write-Output ""
Write-Output "Written to $Out"
