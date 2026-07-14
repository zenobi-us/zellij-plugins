#!/usr/bin/env bats

setup() {
  REPO_ROOT="$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)"
  TEST_ROOT="$(mktemp -d)"
  mkdir -p "${TEST_ROOT}/bin"
}

teardown() {
  rm -rf "${TEST_ROOT}"
}

@test "publish matrix returns sorted target-only entries for next publishing" {
  cat >"${TEST_ROOT}/bin/moon" <<'EOF'
#!/usr/bin/env bash
cat <<'JSON'
{"projects":[
  {"id":"z-target","tasks":{"publish":{}}},
  {"id":"ignored","tasks":{"test":{}}},
  {"id":"a-target","tasks":{"publish":{}}}
]}
JSON
EOF
  chmod +x "${TEST_ROOT}/bin/moon"

  run env PATH="${TEST_ROOT}/bin:${PATH}" \
    "${REPO_ROOT}/.github/tasks/get-publish-matrix" \
    '{"releases_created":"false","prs_created":"true"}' base head

  [ "${status}" -eq 0 ]
  [ "${output}" = '[{"target":"a-target"},{"target":"z-target"}]' ]

  run env PATH="${TEST_ROOT}/bin:${PATH}" \
    "${REPO_ROOT}/.github/tasks/get-publish-matrix" '{}' '' head
  [ "${status}" -ne 0 ]

  run env PATH="${TEST_ROOT}/bin:${PATH}" \
    "${REPO_ROOT}/.github/tasks/get-publish-matrix" '[]' base head
  [ "${status}" -ne 0 ]
}

@test "release branch root skips, later commit uses hotfix, merge-back skips" {
  repo="${TEST_ROOT}/repo"
  git init -q -b main "${repo}"
  git -C "${repo}" config user.email test@example.com
  git -C "${repo}" config user.name Test
  echo root >"${repo}/file"
  git -C "${repo}" add file
  git -C "${repo}" commit -qm root
  root_sha="$(git -C "${repo}" rev-parse HEAD)"
  git -C "${repo}" branch release/0.1
  git -C "${repo}" update-ref refs/remotes/origin/main "${root_sha}"
  git -C "${repo}" update-ref refs/remotes/origin/release/0.1 "${root_sha}"

  run bash -c "cd '${repo}' && '${REPO_ROOT}/.github/tasks/get-release-policy' '${root_sha}' 'release/0.1' 'refs/remotes/origin/main'"
  [ "${status}" -eq 0 ]
  [ "$(jq -r .mode <<<"${output}")" = skip ]
  [ "$(jq -r .reason <<<"${output}")" = release-root ]

  git -C "${repo}" checkout -q release/0.1
  echo hotfix >>"${repo}/file"
  git -C "${repo}" commit -qam hotfix
  hotfix_sha="$(git -C "${repo}" rev-parse HEAD)"
  git -C "${repo}" update-ref refs/remotes/origin/release/0.1 "${hotfix_sha}"

  run bash -c "cd '${repo}' && '${REPO_ROOT}/.github/tasks/get-release-policy' '${hotfix_sha}' 'release/0.1' 'refs/remotes/origin/main'"
  [ "${status}" -eq 0 ]
  [ "$(jq -r .mode <<<"${output}")" = hotfix ]

  git -C "${repo}" checkout -q main
  echo main >"${repo}/main"
  git -C "${repo}" add main
  git -C "${repo}" commit -qm main
  main_sha="$(git -C "${repo}" rev-parse HEAD)"
  git -C "${repo}" update-ref refs/remotes/origin/main "${main_sha}"

  run bash -c "cd '${repo}' && '${REPO_ROOT}/.github/tasks/get-release-policy' '${main_sha}' main 'refs/remotes/origin/main'"
  [ "${status}" -eq 0 ]
  [ "$(jq -r .mode <<<"${output}")" = normal ]

  git -C "${repo}" merge -q --no-ff release/0.1 -m merge-hotfix
  merge_sha="$(git -C "${repo}" rev-parse HEAD)"
  git -C "${repo}" update-ref refs/remotes/origin/main "${merge_sha}"

  run bash -c "cd '${repo}' && '${REPO_ROOT}/.github/tasks/get-release-policy' '${merge_sha}' main 'refs/remotes/origin/main'"
  [ "${status}" -eq 0 ]
  [ "$(jq -r .mode <<<"${output}")" = skip ]
  [ "$(jq -r .reason <<<"${output}")" = hotfix-merge-back ]
}

@test "associated release PR head identifies squash merge-back" {
  repo="${TEST_ROOT}/repo"
  git init -q -b main "${repo}"
  git -C "${repo}" config user.email test@example.com
  git -C "${repo}" config user.name Test
  echo root >"${repo}/file"
  git -C "${repo}" add file
  git -C "${repo}" commit -qm root
  sha="$(git -C "${repo}" rev-parse HEAD)"
  git -C "${repo}" update-ref refs/remotes/origin/main "${sha}"
  printf 'release/0.1\n' >"${TEST_ROOT}/heads"

  run bash -c "cd '${repo}' && '${REPO_ROOT}/.github/tasks/get-release-policy' '${sha}' main 'refs/remotes/origin/main' '${TEST_ROOT}/heads'"
  [ "${status}" -eq 0 ]
  [ "$(jq -r .mode <<<"${output}")" = skip ]
  [ "$(jq -r .reason <<<"${output}")" = hotfix-merge-back-pr ]
}

@test "workflow contract uses channel and immutable source identity" {
  release="${REPO_ROOT}/.github/workflows/release.yml"
  publish="${REPO_ROOT}/.github/workflows/publish.yml"
  checkout_source="${REPO_ROOT}/.github/tasks/checkout-publish-source"

  grep -q 'client_payload.channel' "${publish}"
  grep -q 'client_payload.source_branch' "${publish}"
  grep -q 'DISPATCH_SOURCE_SHA' "${publish}"
  grep -q 'DISPATCH_SOURCE_BRANCH' "${publish}"
  grep -q 'inputs.channel' "${publish}"
  grep -q 'github-actions\[bot\]' "${publish}"
  grep -q 'merge-base --is-ancestor' "${checkout_source}"
  grep -q 'checkout --detach' "${checkout_source}"
  grep -q '"channel":' "${release}"
  grep -q '"source_branch":' "${release}"

  run grep -qE 'client_payload\.(tag|version|release_tag|mode)' "${publish}"
  [ "${status}" -ne 0 ]
  run grep -q 'client_payload.source_sha || github.sha' "${publish}"
  [ "${status}" -ne 0 ]
  run grep -q 'client_payload.source_branch || github.ref_name' "${publish}"
  [ "${status}" -ne 0 ]
  run grep -qE 'inputs\.(tag|ref)' "${publish}"
  [ "${status}" -ne 0 ]
  run grep -qE '"(tag|version|release_tag|mode)":' "${release}"
  [ "${status}" -ne 0 ]
}

@test "publish task preserves shell target variable through Moon token expansion" {
  expanded="$(cd "${REPO_ROOT}" && moon project zellij-tabbar --json | jq -r '.tasks.publish.script')"

  [[ "${expanded}" == *'[[ "${target}" == zellij-tabbar ]]'* ]]
  [[ "${expanded}" != *'Unexpected publish target '\''zellij-tabbar:publish'\'''* ]]
}
