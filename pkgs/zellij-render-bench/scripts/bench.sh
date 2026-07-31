#!/usr/bin/env bash
set -euo pipefail

pkg_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_root="$(cd "$pkg_root/../.." && pwd)"
layout="${LAYOUT:-demo/one-plugin-grid.kdl}"
seconds="${BENCH_SECONDS:-60}"

usage() {
  cat <<'USAGE'
Usage: pkgs/zellij-render-bench/scripts/bench.sh <micro|build|launch|measure>

Commands:
  micro     Run the direct renderer benchmark.
  build     Build the WASM demo plugin.
  launch    Launch the Zellij demo layout. Set LAYOUT=demo/ten-plugin-grid.kdl for stress.
  measure   Measure the newest zellij process with pidstat and perf stat when installed.

Env: ITERATIONS=1000, LAYOUT=demo/one-plugin-grid.kdl, BENCH_SECONDS=60, ZELLIJ_PID=<pid>
USAGE
}

build_plugin() {
  rustup target add wasm32-wasip1 >/dev/null 2>&1 || true
  (cd "$pkg_root" && CARGO_TARGET_DIR="$PWD/target" cargo build --release --target wasm32-wasip1)
}

case "${1:-}" in
  micro)
    host_target="$(rustc -vV | awk '/^host:/ {print $2}')"
    (cd "$repo_root" && cargo run --release --target "$host_target" -p zellij-template-render --example render_100_buttons -- "${ITERATIONS:-1000}")
    ;;
  build)
    build_plugin
    ;;
  launch)
    build_plugin
    command -v zellij >/dev/null || { echo "zellij is required" >&2; exit 1; }
    (cd "$pkg_root" && zellij --new-session-with-layout "$PWD/$layout")
    ;;
  measure)
    pid="${ZELLIJ_PID:-$(pgrep -n zellij || true)}"
    [[ -n "$pid" ]] || { echo "no zellij process found" >&2; exit 1; }
    echo "zellij pid=$pid seconds=$seconds"
    if command -v pidstat >/dev/null; then
      pidstat -rud -h -p "$pid" 1 "$seconds"
    else
      echo "skip pidstat: command not found" >&2
    fi
    if command -v perf >/dev/null; then
      perf stat -p "$pid" -e task-clock,cycles,instructions,context-switches,page-faults -- sleep "$seconds"
    else
      echo "skip perf: command not found" >&2
    fi
    ;;
  *)
    usage
    exit 2
    ;;
esac
