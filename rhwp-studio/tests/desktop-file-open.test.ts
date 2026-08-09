import test from 'node:test';
import assert from 'node:assert/strict';

import {
  OPEN_PATH_EVENT,
  setupDesktopFileOpen,
} from '../src/core/desktop-file-open.ts';

test('이벤트 이름은 Rust 쪽 OPEN_PATH_EVENT 와 같아야 한다', () => {
  // 셸(src-tauri/src/file_open.rs)의 상수와 문자열이 어긋나면 두 번째 인스턴스의
  // 문서 열기가 조용히 사라진다 — 양쪽을 같은 값으로 못박아 둔다.
  assert.equal(OPEN_PATH_EVENT, 'rhwp://open-path');
});

test('데스크톱 런타임이 아니면 아무 것도 하지 않고 false 를 반환한다', async () => {
  // node 테스트 환경에는 window/__TAURI_INTERNALS__ 가 없다 = 웹 빌드와 같은 조건.
  let emitted = 0;
  let errors = 0;

  const result = await setupDesktopFileOpen(() => { emitted += 1; }, () => { errors += 1; });

  // false 를 반환해야 부팅 시 자동 저장 복구 제안이 정상적으로 진행된다.
  assert.equal(result, false);
  assert.equal(emitted, 0);
  assert.equal(errors, 0);
});
