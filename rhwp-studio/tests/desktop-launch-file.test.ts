import test from 'node:test';
import assert from 'node:assert/strict';

import {
  installDesktopLaunchFileHandling,
  type DesktopLaunchFileBridge,
} from '../src/command/desktop-launch-file.ts';
import type { OpenDocumentBytesPayload, PwaFileHandlingCallbacks } from '../src/command/pwa-file-handling.ts';
import type { FileSystemFileHandleLike } from '../src/command/file-system-access.ts';

function baseName(path: string): string {
  return /[^\\/]+$/.exec(path)?.[0] ?? path;
}

function createPathHandle(path: string): FileSystemFileHandleLike {
  return {
    kind: 'file' as const,
    name: baseName(path),
    async getFile() {
      return new File(['fixture'], baseName(path), { type: 'application/x-hwp' });
    },
    async createWritable() {
      throw new Error('파일 연결 열기에서 쓰기가 호출되면 안 된다');
    },
  };
}

function createCallbacks() {
  const opened: OpenDocumentBytesPayload[] = [];
  const unsupported: string[] = [];
  const errors: unknown[] = [];
  const callbacks: PwaFileHandlingCallbacks = {
    openDocumentBytes(payload) { opened.push(payload); },
    notifyUnsupportedFile(fileName) { unsupported.push(fileName); },
    notifyError(error) { errors.push(error); },
  };
  return { callbacks, opened, unsupported, errors };
}

function createBridge(initial: string[]) {
  let listener: ((paths: string[]) => void) | null = null;
  const calls: string[] = [];
  const bridge: DesktopLaunchFileBridge = {
    async takeLaunchFiles() {
      calls.push('take');
      return initial;
    },
    async onLaunchFiles(handler) {
      calls.push('listen');
      listener = handler;
    },
    createHandle: createPathHandle,
  };
  return { bridge, calls, emit: (paths: string[]) => listener?.(paths) };
}

test('시작 큐에 있던 hwp 경로를 열고, 리스너를 take보다 먼저 등록한다', async () => {
  const { callbacks, opened } = createCallbacks();
  const { bridge, calls } = createBridge(['C:\\문서\\보고서.hwp']);

  await installDesktopLaunchFileHandling(callbacks, bridge);

  assert.deepEqual(calls, ['listen', 'take']);
  assert.equal(opened.length, 1);
  assert.equal(opened[0].fileName, '보고서.hwp');
  assert.ok(opened[0].bytes.length > 0);
});

test('시작 큐가 비어 있으면 아무것도 열지 않는다', async () => {
  const { callbacks, opened, errors } = createCallbacks();
  const { bridge } = createBridge([]);

  await installDesktopLaunchFileHandling(callbacks, bridge);

  assert.equal(opened.length, 0);
  assert.equal(errors.length, 0);
});

test('실행 중 이벤트로 도착한 경로도 연다 (macOS Opened)', async () => {
  const { callbacks, opened } = createCallbacks();
  const { bridge, emit } = createBridge([]);

  await installDesktopLaunchFileHandling(callbacks, bridge);
  emit(['/Users/me/계약서.hwpx']);
  await new Promise((resolve) => setTimeout(resolve, 0));

  assert.equal(opened.length, 1);
  assert.equal(opened[0].fileName, '계약서.hwpx');
});

test('지원하지 않는 확장자는 열지 않고 안내한다', async () => {
  const { callbacks, opened, unsupported } = createCallbacks();
  const { bridge } = createBridge(['/tmp/메모.txt']);

  await installDesktopLaunchFileHandling(callbacks, bridge);

  assert.equal(opened.length, 0);
  assert.deepEqual(unsupported, ['메모.txt']);
});
