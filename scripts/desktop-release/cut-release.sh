#!/usr/bin/env sh
# RHWP Studio 데스크톱 릴리스 태그를 끊는다.
#
# 사용: scripts/desktop-release/cut-release.sh <버전>   (예: 0.9.2)
#
# 하는 일:
#   1. rhwp-studio/package.json과 src-tauri/tauri.conf.json의 version을 <버전>으로 맞춘다
#   2. 버전 커밋을 만들고 studio-v<버전> 태그를 붙인다
#   3. 현재 브랜치와 태그를 fork(kseungyong/rhwp)로 push한다
#      → GitHub Actions(desktop-release.yml)가 빌드·포털 게시를 이어받는다
#
# 자동 업데이트는 semver 비교로 동작하므로 버전은 직전 릴리스보다 커야 한다.
set -eu

ver="${1:?사용법: cut-release.sh <버전> (예: 0.9.2)}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

if [ -n "$(git status --porcelain)" ]; then
  echo "작업 트리가 깨끗하지 않습니다. 커밋/스태시 후 다시 실행하세요." >&2
  exit 1
fi

node - "$ver" <<'EOF'
const { readFileSync, writeFileSync } = require('fs');
const ver = process.argv[2];
if (!/^\d+\.\d+\.\d+$/.test(ver)) {
  console.error(`버전 형식이 올바르지 않습니다: ${ver} (예: 0.9.2)`);
  process.exit(1);
}
for (const file of ['rhwp-studio/package.json', 'rhwp-studio/src-tauri/tauri.conf.json']) {
  const text = readFileSync(file, 'utf-8');
  const updated = text.replace(/("version":\s*")[^"]+(")/, `$1${ver}$2`);
  writeFileSync(file, updated);
  console.log(`${file} → ${ver}`);
}
EOF

git add rhwp-studio/package.json rhwp-studio/src-tauri/tauri.conf.json
if git diff --cached --quiet; then
  echo "버전이 이미 ${ver} 입니다 — 태그만 진행합니다."
else
  git commit -m "chore(release): rhwp-studio ${ver}

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
fi

git tag "studio-v${ver}"
git push fork "$(git rev-parse --abbrev-ref HEAD)" "studio-v${ver}"
echo "완료 — fork의 GitHub Actions에서 빌드·게시가 진행됩니다:"
echo "  https://github.com/kseungyong/rhwp/actions"
