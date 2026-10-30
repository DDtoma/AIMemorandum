#!/bin/bash
# AM-002 Phase 0 清单第 2 条：Linux 内存采样。
# 用法: scripts/measure-memory.sh <pid> [秒数=30]
set -euo pipefail
pid="$1"; seconds="${2:-30}"
if [ ! -d "/proc/$pid" ]; then echo "进程 $pid 不存在" >&2; exit 1; fi
max=0; sum=0; n=0
for _ in $(seq "$seconds"); do
    rss=$(awk '/VmRSS/{print $2}' "/proc/$pid/status")
    [ -z "$rss" ] && break
    sum=$((sum + rss)); n=$((n + 1))
    [ "$rss" -gt "$max" ] && max=$rss
    sleep 1
done
[ "$n" -eq 0 ] && { echo "没有采到样本" >&2; exit 1; }
printf 'samples=%d avg=%.1f MB peak=%.1f MB\n' "$n" "$(echo "$sum/$n/1024" | bc -l)" "$(echo "$max/1024" | bc -l)"
echo "判定口径：空闲 ≤60 MB，窗口模式 ≤80 MB（见 phase0-checklist.md 第 2 条）"
