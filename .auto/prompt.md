# Autoresearch: optimize `zellij-template-render`

## Objective

Reduce render time and resource use for `zellij-template-render`.

The fixed workload renders a 10×10 grid of `Button` nodes through MiniJinja, `Flex`, layout, clipping, and hitbox generation.

## Metrics

- **Primary**: `render_p95_us` (microseconds, lower is better).
- **Secondary**: `render_p50_us`, `render_p99_us`, `render_max_us`, `render_worst_us`.
- **Run metadata**: `benchmark_runs`, `benchmark_iterations`.

Use `render_p95_us` as the keep-or-discard metric.

## How to Run

Run:

```bash
./.auto/measure.sh
```

The script emits `METRIC name=value` lines for autoresearch.

You can tune sample count with environment variables:

```bash
RUNS=5 ITERATIONS=2000 ./.auto/measure.sh
```

## Files in Scope

- `pkgs/zellij-template-render/src/lib.rs` — public renderer entry point.
- `pkgs/zellij-template-render/src/template.rs` — MiniJinja helpers and template tree construction.
- `pkgs/zellij-template-render/src/layout.rs` — `Flex` layout, clipping, and hitbox construction.
- `pkgs/zellij-template-render/src/action.rs` — action token decode path if profiling shows cost there.
- `pkgs/zellij-template-render/examples/render_100_buttons.rs` — fixed microbenchmark workload.
- `pkgs/zellij-render-bench/scripts/measure.sh` — metric parser. Change it only to improve measurement signal.
- `.auto/measure.sh` — autoresearch wrapper. Do not add benchmark logic here.

## Off Limits

- Do not change public renderer behavior to improve the benchmark.
- Do not remove hitbox generation.
- Do not remove ANSI-aware width handling.
- Do not change the 100-button workload unless measurement is broken.
- Do not add new dependencies unless the gain is large and measured.
- Do not optimize the Zellij demo plugin before the renderer path is faster.

## Constraints

- Keep the existing tests passing.
- Prefer deletion and simpler data flow over new abstractions.
- Preserve typed actions and coordinate-matched hitboxes.
- Treat equal performance with less code as a useful win.
- Treat small gains with much more code as a loss.

## Suggested First Experiments

1. Profile allocation hot paths in template tree and layout construction.
2. Look for cloned strings or vectors on the render path.
3. Reuse width calculations where the same text is measured more than once.
4. Reduce per-cell `Option<Action>` churn only if hitbox behavior stays exact.
5. Inspect `Flex` layout loops for repeated scans over the same children.

## What's Been Tried

Nothing yet.
