#!/usr/bin/env bash
set -euo pipefail

ARTIFACT_DIR="${1:-artifacts}"
BRANCH_NAME="${LINUX_BRANCH:-linux}"
SERVER_URL="${GITHUB_SERVER_URL:-https://github.com}"

: "${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"
: "${GITHUB_SHA:?GITHUB_SHA is required}"
: "${GITHUB_RUN_NUMBER:?GITHUB_RUN_NUMBER is required}"
: "${GITHUB_TOKEN:?GITHUB_TOKEN is required}"

if [[ ! -d "$ARTIFACT_DIR" ]]; then
  echo "artifact directory not found: $ARTIFACT_DIR" >&2
  exit 1
fi

branch_dir="$(mktemp -d "${RUNNER_TEMP:-/tmp}/galtransl-linux-branch.XXXXXX")"
git init -b "$BRANCH_NAME" "$branch_dir"

find "$ARTIFACT_DIR" -maxdepth 2 -type f -exec cp -f {} "$branch_dir/" \;

if ! compgen -G "$branch_dir/*" >/dev/null; then
  echo "no artifacts were downloaded" >&2
  exit 1
fi

cat > "$branch_dir/README.md" <<EOF
# GalTransl Linux x86_64 builds

This branch is generated automatically by the **Linux x86_64** GitHub Actions workflow.

- Source commit: \`${GITHUB_SHA}\`
- Workflow run: \`${GITHUB_RUN_NUMBER}\`
- Supported architecture: \`x86_64\`
- Package formats: \`.deb\`, \`.rpm\`, \`.AppImage\`, \`.tar.gz\`

Do not edit files on this branch manually. New builds replace the previous contents.
EOF

cat > "$branch_dir/BUILD_INFO.txt" <<EOF
source_repository=${GITHUB_REPOSITORY}
source_commit=${GITHUB_SHA}
source_ref=${GITHUB_REF_NAME:-unknown}
workflow_run=${GITHUB_RUN_NUMBER}
built_at_utc=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
architecture=x86_64
EOF

(
  cd "$branch_dir"
  find . -maxdepth 1 -type f ! -name 'SHA256SUMS' -printf '%f\0' \
    | sort -z \
    | xargs -0 sha256sum > SHA256SUMS
)

git -C "$branch_dir" config user.name "github-actions[bot]"
git -C "$branch_dir" config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git -C "$branch_dir" add -A
git -C "$branch_dir" commit -m "Linux build ${GITHUB_SHA:0:7} (run ${GITHUB_RUN_NUMBER})"
git -C "$branch_dir" remote add origin "${SERVER_URL}/${GITHUB_REPOSITORY}.git"
git -C "$branch_dir" \
  -c http.extraheader="AUTHORIZATION: bearer ${GITHUB_TOKEN}" \
  push --force origin "HEAD:${BRANCH_NAME}"
