#!/usr/bin/env sh
# GitHub Actions 빌드 산출물을 내려받아 사내 포털에 게시한다.
#
# 사용: scripts/desktop-release/publish-from-ci.sh <버전>   (예: 0.9.2)
#
# 포털(portal.kdsys.co.kr)은 사내 DNS 전용이라 GitHub 러너가 접근할 수 없다.
# 그래서 빌드는 CI가, 게시는 사내 작업 머신(이 스크립트)이 맡는다:
#   1. studio-v<버전> 태그의 desktop-release.yml run을 찾아 끝날 때까지 기다린다
#   2. artifacts(Windows exe·macOS dmg·업데이트 패키지·서명)를 내려받는다
#   3. publish-to-portal.mjs로 포털 다운로드 센터에 올린다 (latest.json은 마지막)
#
# 토큰: PORTAL_UPLOAD_TOKEN 환경변수, 없으면 ~/.tauri/portal-upload-token.txt
set -eu

ver="${1:?사용법: publish-from-ci.sh <버전> (예: 0.9.2)}"
repo="kseungyong/rhwp"
tag="studio-v${ver}"
root="$(cd "$(dirname "$0")/../.." && pwd)"

if [ -z "${PORTAL_UPLOAD_TOKEN:-}" ]; then
  token_file="$HOME/.tauri/portal-upload-token.txt"
  [ -f "$token_file" ] || { echo "PORTAL_UPLOAD_TOKEN도 ${token_file}도 없습니다." >&2; exit 1; }
  PORTAL_UPLOAD_TOKEN="$(cat "$token_file")"
  export PORTAL_UPLOAD_TOKEN
fi

echo "태그 ${tag}의 빌드 run을 찾는 중..."
run_id=""
i=0
while [ -z "$run_id" ]; do
  run_id="$(gh run list -R "$repo" --workflow desktop-release.yml --branch "$tag" \
    --limit 1 --json databaseId --jq '.[0].databaseId // empty')"
  [ -n "$run_id" ] && break
  i=$((i + 1))
  [ "$i" -ge 30 ] && { echo "run이 생기지 않습니다 — 태그 push가 됐는지 확인하세요." >&2; exit 1; }
  sleep 10
done

echo "run ${run_id} 완료 대기 (빌드는 보통 20~40분)..."
gh run watch "$run_id" -R "$repo" --exit-status --interval 60

work_dir="$(mktemp -d /tmp/rhwp-desktop-release.XXXXXX)"
trap 'rm -rf "$work_dir"' EXIT
echo "artifacts 내려받는 중 → ${work_dir}"
gh run download "$run_id" -R "$repo" -D "$work_dir"

node "$root/scripts/desktop-release/publish-to-portal.mjs" --version "$ver" --dir "$work_dir"
