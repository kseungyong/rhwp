/**
 * 데스크톱(Tauri) 파일 연결 열기 — OS가 넘긴 문서 경로를 받아 연다.
 *
 * 설치 시 등록되는 `.hwp`/`.hwpx` 파일 연결로 앱이 실행되면:
 * - Windows/Linux: 경로가 argv로 오고, Tauri 셸(lib.rs)이 시작 시 큐에 쌓아 둔다.
 * - macOS: Apple Event(`RunEvent::Opened`)로 오고, WebView가 뜨기 전이면 역시 큐에
 *   쌓인다. 앱이 이미 떠 있으면 `desktop:launch-files` 이벤트로 바로 온다.
 *
 * 여기서는 초기화 완료 시점에 큐를 한 번 비우고(`take_launch_files`), 이후 도착분은
 * 이벤트 리스너로 받아 PWA launch queue와 동일한 열기 경로(handlePwaLaunchFiles →
 * `open-document-bytes`)로 흘린다. 핸들은 경로 기반 {@link createDesktopFileHandle}라서
 * 저장 시 원본 경로 덮어쓰기도 다이얼로그로 연 문서와 동일하게 동작한다.
 */

import { createDesktopFileHandle } from './desktop-file-system.ts';
import {
  handlePwaLaunchFiles,
  type PwaFileHandlingCallbacks,
} from './pwa-file-handling.ts';
import type { FileSystemFileHandleLike } from './file-system-access.ts';

/** Tauri invoke/event 표면 — 테스트에서 대체 가능하게 좁혀 둔다. */
export interface DesktopLaunchFileBridge {
  /** 셸에 버퍼링된 실행 시 전달 경로를 가져오며 큐를 비운다. */
  takeLaunchFiles(): Promise<string[]>;
  /** 실행 중 도착하는 파일 연결 열기(macOS) 구독. 등록 완료 시 resolve. */
  onLaunchFiles(handler: (paths: string[]) => void): Promise<void>;
  /** 경로 → 파일 핸들. 기본은 fs 플러그인을 쓰는 DesktopFileHandle. */
  createHandle(path: string): FileSystemFileHandleLike;
}

const tauriLaunchFileBridge: DesktopLaunchFileBridge = {
  async takeLaunchFiles() {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<string[]>('take_launch_files');
  },
  async onLaunchFiles(handler) {
    const { listen } = await import('@tauri-apps/api/event');
    await listen<string[]>('desktop:launch-files', (event) => handler(event.payload));
  },
  createHandle(path) {
    return createDesktopFileHandle(path);
  },
};

/**
 * 파일 연결 열기 처리를 설치한다. 호출 측(main.ts)이 데스크톱 런타임 여부를 판정한다.
 *
 * 리스너를 먼저 등록한 뒤 큐를 비운다 — 큐를 비운 직후 도착하는 열기 요청이
 * 리스너 등록 전 틈에 유실되지 않게 하기 위한 순서다.
 */
export async function installDesktopLaunchFileHandling(
  callbacks: PwaFileHandlingCallbacks,
  bridge: DesktopLaunchFileBridge = tauriLaunchFileBridge,
): Promise<void> {
  const openPaths = async (paths: string[]) => {
    if (paths.length === 0) return;
    await handlePwaLaunchFiles(
      { files: paths.map((path) => bridge.createHandle(path)) },
      callbacks,
    );
  };
  await bridge.onLaunchFiles((paths) => {
    void openPaths(paths).catch((error) => callbacks.notifyError(error));
  });
  await openPaths(await bridge.takeLaunchFiles());
}
