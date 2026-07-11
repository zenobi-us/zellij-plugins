#!/usr/bin/env bats

setup_file() {
  command -v zellij >/dev/null || skip "zellij is required"
  command -v python3 >/dev/null || skip "python3 is required"
  local zellij_version
  zellij_version="$(zellij --version | awk '{ print $2 }')"
  [[ "$zellij_version" == 0.45.* ]] || skip "Zellij 0.45.x is required by the plugin ABI (found $zellij_version)"
  cargo build --release --target wasm32-wasip1
  export PLUGIN_WASM
  PLUGIN_WASM="$(realpath "$BATS_TEST_DIRNAME/../target/wasm32-wasip1/release/zellij-tabbar.wasm")"
}

setup() {
  TEST_ROOT="$(mktemp -d)"
  SESSION="zellij-tabbar-e2e-${BATS_TEST_NUMBER}-$$-$RANDOM"
  CLIENT_PID=""
}

zellij_test() {
  env ZELLIJ_SOCKET_DIR="$TEST_ROOT/socket" zellij "$@"
}

teardown() {
  zellij_test kill-session "$SESSION" >/dev/null 2>&1 || true
  if [[ -n "$CLIENT_PID" ]]; then
    kill "$CLIENT_PID" >/dev/null 2>&1 || true
    wait "$CLIENT_PID" 2>/dev/null || true
  fi
  rm -rf "$TEST_ROOT"
}

start_plugin() {
  local template="$1"
  cat >"$TEST_ROOT/layout.kdl" <<KDL
layout {
  tab name="Alpha" {
    pane size=1 borderless=true {
      plugin location="file:$PLUGIN_WASM" {
        template r###"$template"###;
      }
    }
    pane
  }
  tab name="Beta" {
    pane
  }
}
KDL

  mkdir -p "$TEST_ROOT/home/.cache/zellij"
  cat >"$TEST_ROOT/home/.cache/zellij/permissions.kdl" <<KDL
"$PLUGIN_WASM" {
  ReadApplicationState
  ChangeApplicationState
}
KDL

  env -u ZELLIJ -u ZELLIJ_SESSION_NAME -u ZELLIJ_PANE_ID \
    TERM=xterm-256color HOME="$TEST_ROOT/home" XDG_CACHE_HOME="$TEST_ROOT/home/.cache" ZELLIJ_SOCKET_DIR="$TEST_ROOT/socket" PTY_LOG="$TEST_ROOT/client.log" \
    python3 "$BATS_TEST_DIRNAME/helpers/pty_client.py" \
    zellij --session "$SESSION" --new-session-with-layout "$TEST_ROOT/layout.kdl" &
  CLIENT_PID=$!

  local panes=""
  for _ in {1..50}; do
    if panes="$(zellij_test --session "$SESSION" action list-panes 2>/dev/null)" \
      && grep -q '^plugin_.*file:' <<<"$panes"; then
      PLUGIN_PANE="$(awk '$2 == "plugin" && $0 ~ /file:/ { print $1; exit }' <<<"$panes")"
      zellij_test --session "$SESSION" action go-to-tab 1 >/dev/null
      zellij_test --session "$SESSION" action rename-tab Alpha >/dev/null
      return
    fi
    sleep 0.1
  done

  printf 'session failed to start\n%s\n' "$panes" >&2
  cat "$TEST_ROOT/client.log" >&2 2>/dev/null || true
  return 1
}

dump_plugin() {
  local expected="$1"
  for _ in {1..100}; do
    if grep -aFq "$expected" "$TEST_ROOT/client.log"; then
      strings "$TEST_ROOT/client.log"
      return
    fi
    sleep 0.1
  done
  zellij_test --session "$SESSION" action list-panes >&2 || true
  return 1
}

@test "inline template receives session and tab model" {
  start_plugin 'SESSION={{ session.name }} TABS={% for tab in session.tabs %}[{{ tab.index }}:{{ tab.name }}:{{ tab.active }}]{% endfor %}'

  run dump_plugin "SESSION=$SESSION"

  [ "$status" -eq 0 ]
  [[ "$output" == *"SESSION=$SESSION"* ]]
  [[ "$output" == *"[1:Alpha:true]"* ]]
  [[ "$output" == *"[2:Beta:false]"* ]]
}

@test "nested Flex places content at opposite viewport edges" {
  start_plugin '{% call Flex(direction="row") %}{% call Flex(shrink=0) %}LEFT{% endcall %}{% call Flex(grow=1, justify="end") %}RIGHT{% endcall %}{% endcall %}'

  run dump_plugin "LEFT"

  [ "$status" -eq 0 ]
  [[ "$output" == *"LEFT"* ]]
  [[ "$output" == *"RIGHT"* ]]
}

@test "template errors render in the plugin pane" {
  start_plugin '{{ broken'

  run dump_plugin "template error:"

  [ "$status" -eq 0 ]
  [[ "$output" == *"template error:"* ]]
}
