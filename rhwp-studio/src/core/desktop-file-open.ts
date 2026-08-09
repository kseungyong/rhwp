/**
 * 파일 연결(.hwp/.hwpx 더블클릭)로 전달된 문서를 연다.
 *
 * 셸(Rust)이 두 경로로 경로를 넘긴다 — 자세한 배경은 `src-tauri/src/file_open.rs`.
 * - **최초 실행**: 대기열에 담아 두고 여기서 `take_pending_open_path` 로 가져간다.
 *   부팅 시점에는 이벤트 리스너가 아직 없으므로 이벤트가 아니라 대기열이어야 한다.
 * - **이미 실행 중**: single-instance 가 넘긴 argv 를 `rhwp://open-path` 이벤트로 받는다.
 *
 * 어느 쪽이든 문서 바이트를 읽어 `open-document-bytes` 로 넘긴다. `skipUnsavedGuard` 를
 * 붙이지 않으므로, 편집 중인 문서가 있으면 기존 저장 확인 흐름이 그대로 걸린다.
 */

// node --test 에서 별칭(@/)이 해석되지 않으므로 테스트 대상 모듈은 상대 경로를 쓴다.
import {
  createDesktopFileHandle,
  isDesktopRuntime,
} from '../command/desktop-file-system.ts';
import { readFileFromHandle } from '../command/file-system-access.ts';

/** 실행 중인 창에 새 문서 경로를 알리는 이벤트 이름 (Rust 쪽 `OPEN_PATH_EVENT` 와 같은 값). */
export const OPEN_PATH_EVENT = 'rhwp://open-path';

export interface OpenDocumentPayload {
  bytes: Uint8Array;
  fileName: string;
  fileHandle: ReturnType<typeof createDesktopFileHandle>;
}

export type EmitOpenDocument = (payload: OpenDocumentPayload) => void;

/** 경로 하나를 읽어 문서 열기 흐름으로 넘긴다. */
export async function openDocumentPath(
  path: string,
  emitOpenDocument: EmitOpenDocument,
): Promise<void> {
  const fileHandle = createDesktopFileHandle(path);
  const { bytes, name } = await readFileFromHandle(fileHandle);
  emitOpenDocument({ bytes, fileName: name, fileHandle });
}

/**
 * 데스크톱 셸의 파일 연결 열기를 연결한다. 웹에서는 아무 것도 하지 않는다.
 *
 * **실행 인자로 열 문서가 있으면 `true`** 를 반환한다. 문서 로드가 끝나기를 기다리지 않고
 * "열 문서가 있다"는 사실이 확정되는 즉시 반환한다 — 부팅 시 자동 저장 복구 제안이 이 결과를
 * 기다렸다가, 열릴 문서가 있으면 스스로 물러나게 하기 위해서다. 기다리지 않으면 복구본이
 * 더블클릭한 문서를 덮어쓴다.
 *
 * 실패해도 앱 부팅을 막지 않는다 — 문서를 못 열 뿐 빈 편집기는 정상 동작해야 한다.
 */
export async function setupDesktopFileOpen(
  emitOpenDocument: EmitOpenDocument,
  onError: (error: unknown) => void,
): Promise<boolean> {
  if (!isDesktopRuntime()) return false;

  try {
    const [{ invoke }, { listen }] = await Promise.all([
      import('@tauri-apps/api/core'),
      import('@tauri-apps/api/event'),
    ]);

    await listen<string>(OPEN_PATH_EVENT, (event) => {
      void openDocumentPath(event.payload, emitOpenDocument).catch(onError);
    });

    // 리스너를 먼저 걸고 대기열을 비운다 — 순서를 뒤집으면 그 사이에 도착한 경로를 놓친다.
    const pending = await invoke<string | null>('take_pending_open_path');
    if (!pending) return false;

    void openDocumentPath(pending, emitOpenDocument).catch(onError);
    return true;
  } catch (error) {
    onError(error);
    return false;
  }
}
