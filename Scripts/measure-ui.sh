#!/usr/bin/env bash
# Measures Dawnload's Slint prototype: memory and CPU with the window open and idle, with 500
# demo items updating, and while the list scrolls. macOS and Linux (Scripts/measure-ui.ps1 is
# the Windows twin). Needs a desktop session: the window must be on screen.
#
#   Scripts/measure-ui.sh            # release build, 30 s per scenario
#   SECONDS_PER=60 Scripts/measure-ui.sh
#   BIN=./dawnload-app ./measure-ui.sh   # a prebuilt binary (a CI artifact), no Rust needed
set -euo pipefail
SECONDS_PER=${SECONDS_PER:-30}
WARMUP=${WARMUP:-8}
OS=$(uname -s)

if [ -n "${BIN:-}" ]; then
  BIN=$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")
  [ -x "$BIN" ] || { echo "$BIN is not an executable" >&2; exit 1; }
  OUT=${OUT:-$(dirname "$BIN")/measure-ui-$(uname -s | tr A-Z a-z).md}
else
  cd "$(dirname "$0")/.."
  cargo build --release -q -p dawnload-ui
  BIN=$PWD/target/release/dawnload-app
  OUT=${OUT:-target/measure-ui-$(uname -s | tr A-Z a-z).md}
fi

cpu_seconds() { # cumulative user+system CPU seconds of $1
  if [ "$OS" = "Linux" ]; then
    awk -v hz="$(getconf CLK_TCK)" '{print ($14+$15)/hz}' "/proc/$1/stat"
  else
    ps -o cputime= -p "$1" | awk -F'[:.]' '{ if (NF==4) print $1*3600+$2*60+$3+$4/100; else print $1*60+$2+$3/100 }'
  fi
}
rss_mb() { ps -o rss= -p "$1" | awk '{printf "%.1f", $1/1024}'; }
footprint_mb() { # macOS "Memory" in Activity Monitor; Linux: proportional set size
  if [ "$OS" = "Darwin" ]; then
    footprint "$1" 2>/dev/null | awk '/phys_footprint:/ {v=$2; u=$3; if (u=="KB") v/=1024; if (u=="GB") v*=1024; printf "%.1f", v; exit}'
  else
    awk '/^Pss:/ {s+=$2} END {printf "%.1f", s/1024}' "/proc/$1/smaps_rollup" 2>/dev/null
  fi
}

display_note() { # Linux: which display protocol the app gets (winit prefers Wayland), and the renderer
  [ "$OS" = "Linux" ] || return 0
  if [ -n "${WAYLAND_DISPLAY:-}" ]; then printf ', Wayland'; elif [ -n "${DISPLAY:-}" ]; then printf ', X11'; fi
  [ -n "${XDG_SESSION_TYPE:-}" ] && printf ' (%s session)' "$XDG_SESSION_TYPE"
  [ -n "${SLINT_BACKEND:-}" ] && printf ', SLINT_BACKEND=%s' "$SLINT_BACKEND"
  return 0
}

scenario() { # name, env...
  local name=$1; shift
  local log; log=$(mktemp)
  env "$@" DAWNLOAD_UI_STATS=1 "$BIN" >"$log" 2>&1 &
  local pid=$!
  sleep "$WARMUP"
  local c0; c0=$(cpu_seconds "$pid")
  local peak_rss=0 samples=0 rss_sum=0
  for _ in $(seq "$SECONDS_PER"); do
    sleep 1
    local r; r=$(rss_mb "$pid")
    rss_sum=$(awk -v a="$rss_sum" -v b="$r" 'BEGIN{print a+b}')
    peak_rss=$(awk -v a="$peak_rss" -v b="$r" 'BEGIN{print (b>a)?b:a}')
    samples=$((samples+1))
  done
  local c1; c1=$(cpu_seconds "$pid")
  local fp; fp=$(footprint_mb "$pid")
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null || true
  local cpu; cpu=$(awk -v a="$c0" -v b="$c1" -v t="$SECONDS_PER" 'BEGIN{printf "%.1f", (b-a)/t*100}')
  local rss; rss=$(awk -v s="$rss_sum" -v n="$samples" 'BEGIN{printf "%.1f", s/n}')
  local fps; fps=$(grep -o 'fps=[0-9]*' "$log" | tail -n "$SECONDS_PER" | awk -F= '{s+=$2; n++} END {if (n) printf "%.0f", s/n; else print "-"}')
  local rows; rows=$(grep -o 'rows_patched=[0-9]*' "$log" | tail -n "$SECONDS_PER" | awk -F= '{s+=$2; n++} END {if (n) printf "%.0f", s/n; else print "-"}')
  printf '| %s | %s | %s | %s | %s%% | %s | %s |\n' "$name" "$rss" "$peak_rss" "${fp:--}" "$cpu" "$fps" "$rows" | tee -a "$OUT"
  rm -f "$log"
}

{
  echo "# Dawnload UI measurement"
  echo
  echo "$(uname -srm), $(date -u +%Y-%m-%dT%H:%MZ), ${SECONDS_PER}s per scenario after ${WARMUP}s warm-up, $BIN$(display_note)."
  echo
  echo "| Scenario | RSS avg (MB) | RSS peak (MB) | Footprint/PSS (MB) | CPU (one core) | Frames/s | Rows patched/s |"
  echo "|---|---|---|---|---|---|---|"
} | tee "$OUT"
scenario "Idle, empty list" DAWNLOAD_DEMO_ITEMS=0
scenario "Idle, 500 items updating" DAWNLOAD_DEMO_ITEMS=500
scenario "Scrolling, 500 items updating" DAWNLOAD_DEMO_ITEMS=500 DAWNLOAD_UI_AUTOSCROLL=14
scenario "Scrolling, 500 items, eased rows" DAWNLOAD_DEMO_ITEMS=500 DAWNLOAD_UI_AUTOSCROLL=14 DAWNLOAD_UI_EASE_ROWS=1
echo
echo "Written to $OUT"
