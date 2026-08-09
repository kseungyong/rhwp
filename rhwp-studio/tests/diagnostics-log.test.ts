import test from 'node:test';
import assert from 'node:assert/strict';

import {
  MAX_FIELD_LENGTH,
  formatLifecycle,
  installDiagnosticsLog,
  logLifecycle,
  reduceArg,
  resetDiagnosticsLogForTest,
  truncate,
  type DiagnosticsSink,
} from '../src/core/diagnostics-log.ts';

interface FakeSink extends DiagnosticsSink {
  lines: string[];
}

function createSink(): FakeSink {
  const lines: string[] = [];
  return {
    lines,
    info: (m) => { lines.push(`info: ${m}`); },
    warn: (m) => { lines.push(`warn: ${m}`); },
    error: (m) => { lines.push(`error: ${m}`); },
  };
}

test('긴 문자열은 잘리고 원래 길이가 남는다', () => {
  const long = 'ㄱ'.repeat(MAX_FIELD_LENGTH + 50);
  const out = truncate(long);
  assert.ok(out.length < long.length);
  assert.ok(out.endsWith(`…(총 ${long.length}자)`));
  assert.equal(truncate('짧은 값'), '짧은 값');
});

test('임의 객체는 종류만 남고 내용은 남지 않는다', () => {
  // 문서 본문이 담긴 객체가 통째로 직렬화되면 안 된다.
  const payload = { 본문: '대외비 문서 내용', 표: [1, 2, 3] };
  const out = reduceArg(payload);
  assert.equal(out, '[Object]');
  assert.ok(!out.includes('대외비'));
});

test('Error 는 이름과 메시지만 남기고 메시지도 잘린다', () => {
  const err = new Error('X'.repeat(MAX_FIELD_LENGTH + 10));
  const out = reduceArg(err);
  assert.ok(out.startsWith('Error: '));
  assert.ok(out.includes('…(총 '));
});

test('원시값은 그대로 남는다', () => {
  assert.equal(reduceArg(42), '42');
  assert.equal(reduceArg(true), 'true');
  assert.equal(reduceArg(null), 'null');
  assert.equal(reduceArg(undefined), 'undefined');
});

test('라이프사이클 한 줄은 key=value 로 구성된다', () => {
  assert.equal(
    formatLifecycle('document-open', { format: 'hwpx', bytes: 1234, pages: 3 }),
    '[document-open] format=hwpx bytes=1234 pages=3',
  );
  assert.equal(formatLifecycle('session'), '[session]');
});

test('설치 후 console.error/warn 은 전송되고 info/log/debug 는 전송되지 않는다', async () => {
  resetDiagnosticsLogForTest();
  const sink = createSink();
  const saved = { error: console.error, warn: console.warn, info: console.info, log: console.log };

  assert.equal(await installDiagnosticsLog(sink), true);
  try {
    console.error('오류 발생');
    console.warn('경고 발생');
    console.info('정보');
    console.log('로그');
  } finally {
    Object.assign(console, saved);
  }

  assert.deepEqual(sink.lines, ['error: 오류 발생', 'warn: 경고 발생']);
  resetDiagnosticsLogForTest();
});

test('전송 중 발생한 오류는 다시 퍼널을 타지 않는다', async () => {
  resetDiagnosticsLogForTest();
  let calls = 0;
  const exploding: DiagnosticsSink = {
    info: () => { calls += 1; throw new Error('sink 고장'); },
    warn: () => { calls += 1; throw new Error('sink 고장'); },
    error: () => { calls += 1; throw new Error('sink 고장'); },
  };
  const saved = { error: console.error, warn: console.warn };

  await installDiagnosticsLog(exploding);
  try {
    // sink 가 던져도 앱이 죽지 않아야 하고, 재귀로 무한히 불리지도 않아야 한다.
    console.error('한 번');
  } finally {
    Object.assign(console, saved);
  }

  assert.equal(calls, 1);
  resetDiagnosticsLogForTest();
});

test('라이프사이클 이벤트는 INFO 로 전송된다', async () => {
  resetDiagnosticsLogForTest();
  const sink = createSink();
  await installDiagnosticsLog(sink);

  logLifecycle('document-save', { format: 'hwp', bytes: 999 });

  assert.deepEqual(sink.lines, ['info: [document-save] format=hwp bytes=999']);
  resetDiagnosticsLogForTest();
});

test('데스크톱 런타임이 아니면 설치하지 않는다', async () => {
  resetDiagnosticsLogForTest();
  // sink 를 주입하지 않으면 웹 조건(window/__TAURI_INTERNALS__ 없음)에서 false 여야 한다.
  assert.equal(await installDiagnosticsLog(), false);
});
