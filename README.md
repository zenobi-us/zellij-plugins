# Zellij plugins

Plugins and shared libraries for [Zellij](https://zellij.dev/).

## Packages

| Package | Description |
|---|---|
| [`zellij-render-bench`](pkgs/zellij-render-bench) | Demo plugin for measuring renderer cost in Zellij |
| [`zellij-tabbar`](pkgs/zellij-tabbar) | Focusable, template-driven tab bar plugin |
| [`zellij-template-render`](pkgs/zellij-template-render) | Reusable template, terminal layout, and typed hitbox renderer |

Package READMEs contain installation, configuration, and usage documentation.

## Development

This repository uses [Moon](https://moonrepo.dev/) to run workspace tasks.

```bash
moon run repo:build
moon run repo:check
moon run repo:test
moon run repo:e2e
```

Run a task for one package:

```bash
moon run zellij-tabbar:build
```

Measure renderer impact:

```bash
moon run zellij-render-bench:bench-micro
moon run zellij-render-bench:demo
moon run zellij-render-bench:bench-measure
```

Install locally:

```bash
moon run repo:install
```

## Structure

```text
pkgs/
├── zellij-render-bench/
├── zellij-tabbar/
└── zellij-template-render/
```
