# Measures Dawnload's Slint prototype on Windows: memory and CPU with the window open and idle,
# with 500 demo items updating, and while the list scrolls. Twin of Scripts/measure-ui.sh.
#
#   pwsh Scripts/measure-ui.ps1                      # builds, then measures
#   pwsh Scripts/measure-ui.ps1 -SecondsPer 60
#   powershell -File measure-ui.ps1 -Bin .\dawnload-app.exe   # a prebuilt binary, no Rust needed
param(
    [int]$SecondsPer = 30,
    [int]$Warmup = 8,
    [string]$Bin = "",
    [string]$Out = ""
)
$ErrorActionPreference = "Stop"

if ($Bin) {
    $Bin = (Resolve-Path $Bin).Path
    if (-not $Out) { $Out = Join-Path (Split-Path $Bin) "measure-ui-windows.md" }
} else {
    Set-Location (Join-Path $PSScriptRoot "..")
    cargo build --release -q -p dawnload-ui
    if ($LASTEXITCODE -ne 0) { throw "build failed" }
    $Bin = (Resolve-Path "target/release/dawnload-app.exe").Path
    if (-not $Out) { $Out = "target/measure-ui-windows.md" }
}

function Scenario([string]$Name, [hashtable]$Vars) {
    # The app's stats go to a file, not a pipe: reading a pipe that has nothing yet would block.
    $statsFile = [System.IO.Path]::GetTempFileName()
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = "cmd.exe"
    $psi.Arguments = "/c `"`"$Bin`" 2> `"$statsFile`"`""
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.EnvironmentVariables["DAWNLOAD_UI_STATS"] = "1"
    foreach ($k in $Vars.Keys) { $psi.EnvironmentVariables[$k] = $Vars[$k] }
    $shell = [System.Diagnostics.Process]::Start($psi)
    $exe = [System.IO.Path]::GetFileNameWithoutExtension($Bin)
    $proc = $null
    for ($i = 0; $i -lt 50 -and -not $proc; $i++) {
        Start-Sleep -Milliseconds 100
        $proc = Get-CimInstance Win32_Process -Filter "ParentProcessId = $($shell.Id) AND Name = '$exe.exe'" |
            ForEach-Object { Get-Process -Id $_.ProcessId } | Select-Object -First 1
    }
    if (-not $proc) { throw "$exe didn't start" }
    Start-Sleep -Seconds $Warmup
    $proc.Refresh()
    $cpu0 = $proc.TotalProcessorTime.TotalSeconds
    $ws = @(); $private = @()
    for ($i = 0; $i -lt $SecondsPer; $i++) {
        Start-Sleep -Seconds 1
        $proc.Refresh()
        $ws += $proc.WorkingSet64 / 1MB
        $private += $proc.PrivateMemorySize64 / 1MB
    }
    $proc.Refresh()
    $cpu1 = $proc.TotalProcessorTime.TotalSeconds
    Stop-Process -Id $proc.Id -Force
    Start-Sleep -Milliseconds 300
    $stats = Get-Content -Path $statsFile -ErrorAction SilentlyContinue
    Remove-Item -Path $statsFile -ErrorAction SilentlyContinue
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
