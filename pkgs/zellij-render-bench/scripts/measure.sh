#!/usr/bin/env bash
set -euo pipefail

pkg_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runs="${RUNS:-3}"
iterations="${ITERATIONS:-1000}"

median() {
  sort -n | awk '
    { values[NR] = $1 }
    END {
      if (NR == 0) exit 1
      if (NR % 2) print values[(NR + 1) / 2]
      else print int((values[NR / 2] + values[NR / 2 + 1]) / 2)
    }'
}

p50_values=()
p95_values=()
p99_values=()
max_values=()

for run in $(seq 1 "$runs"); do
  output="$(ITERATIONS="$iterations" "$pkg_root/scripts/bench.sh" micro 2>&1)" || {
    printf '%s\n' "$output" >&2
    exit 1
  }
  printf '%s\n' "$output" >&2

  p50="$(awk -F= '/^p50=/{ sub(/us$/, "", $2); print $2 }' <<<"$output")"
  p95="$(awk -F= '/^p95=/{ sub(/us$/, "", $2); print $2 }' <<<"$output")"
  p99="$(awk -F= '/^p99=/{ sub(/us$/, "", $2); print $2 }' <<<"$output")"
  max="$(awk -F= '/^max=/{ sub(/us$/, "", $2); print $2 }' <<<"$output")"

  if [[ -z "$p50" || -z "$p95" || -z "$p99" || -z "$max" ]]; then
    echo "failed to parse render benchmark output" >&2
    exit 1
  fi

  p50_values+=("$p50")
  p95_values+=("$p95")
  p99_values+=("$p99")
  max_values+=("$max")
done

p50_median="$(printf '%s\n' "${p50_values[@]}" | median)"
p95_median="$(printf '%s\n' "${p95_values[@]}" | median)"
p99_median="$(printf '%s\n' "${p99_values[@]}" | median)"
max_median="$(printf '%s\n' "${max_values[@]}" | median)"
max_worst="$(printf '%s\n' "${max_values[@]}" | sort -n | tail -1)"

cat <<METRICS
METRIC render_p95_us=$p95_median
METRIC render_p50_us=$p50_median
METRIC render_p99_us=$p99_median
METRIC render_max_us=$max_median
METRIC render_worst_us=$max_worst
METRIC benchmark_runs=$runs
METRIC benchmark_iterations=$iterations
METRICS
