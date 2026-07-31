# zellij-render-bench

Demo plugin for measuring `zellij-template-render` cost inside Zellij.

The plugin renders 100 buttons in a 10×10 grid. It records render time inside the plugin and shows rolling samples in the pane.

## Build

```bash
moon run zellij-render-bench:build
```

## Run one plugin with nine idle panes

```bash
moon run zellij-render-bench:demo
```

## Run ten plugin panes

```bash
LAYOUT=demo/ten-plugin-grid.kdl moon run zellij-render-bench:demo
```

## Measure

Run the package tasks:

```bash
moon run zellij-render-bench:bench-micro
moon run zellij-render-bench:demo
moon run zellij-render-bench:bench-measure
```

`measure` uses `pidstat` and `perf` when they are installed.

You can also run the package script directly:

```bash
pkgs/zellij-render-bench/scripts/bench.sh micro
pkgs/zellij-render-bench/scripts/bench.sh launch
pkgs/zellij-render-bench/scripts/bench.sh measure
```
