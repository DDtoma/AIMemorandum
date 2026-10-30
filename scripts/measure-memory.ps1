# AM-002 Phase 0 清单第 2 条：Windows 内存采样。
# 用法: powershell -File scripts/measure-memory.ps1 -Pid <pid> [-Seconds 30]
param([Parameter(Mandatory=$true)][int]$Pid, [int]$Seconds = 30)
$proc = Get-Process -Id $Pid -ErrorAction Stop
$max = 0L; $sum = 0L; $n = 0
foreach ($i in 1..$Seconds) {
    $proc.Refresh()
    if ($proc.HasExited) { break }
    $ws = $proc.WorkingSet64
    $sum += $ws; $n += 1
    if ($ws -gt $max) { $max = $ws }
    Start-Sleep -Seconds 1
}
if ($n -eq 0) { Write-Error "没有采到样本"; exit 1 }
$avgMB = [math]::Round($sum / $n / 1MB, 1)
$maxMB = [math]::Round($max / 1MB, 1)
Write-Output "samples=$n avg=$avgMB MB peak=$maxMB MB"
Write-Output "判定口径：空闲 ≤60 MB，窗口模式 ≤80 MB（见 phase0-checklist.md 第 2 条）"
