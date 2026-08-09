/**
 * `.hwp`/`.hwpx` 파일 연결 켜고 끄기 (데스크톱 전용).
 *
 * 설치 관리자가 연결을 무조건 등록하지 않으므로, 사용자가 환경 설정에서 직접 켠다.
 * 배경과 레지스트리 처리는 `src-tauri/src/file_association.rs`.
 *
 * 웹에서는 지원하지 않는다 — `isFileAssociationSupported()` 가 false 를 돌려주고
 * 환경 설정에서 해당 항목을 감춘다.
 */

// node --test 에서 별칭(@/)이 해석되지 않으므로 상대 경로를 쓴다.
import { isDesktopRuntime } from '../command/desktop-file-system.ts';

async function invokeCommand<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

/** 이 환경에서 파일 연결을 바꿀 수 있는지. 웹이면 항상 false. */
export async function isFileAssociationSupported(): Promise<boolean> {
  if (!isDesktopRuntime()) return false;
  try {
    return await invokeCommand<boolean>('file_association_supported');
  } catch {
    return false;
  }
}

/** 현재 이 앱으로 연결돼 있는지. 확인에 실패하면 false 로 본다(꺼짐으로 표시). */
export async function isFileAssociationEnabled(): Promise<boolean> {
  if (!isDesktopRuntime()) return false;
  try {
    return await invokeCommand<boolean>('file_association_enabled');
  } catch {
    return false;
  }
}

/**
 * 파일 연결을 켜거나 끈다.
 *
 * 실패는 삼키지 않는다 — 사용자가 명시적으로 누른 동작이므로 실패 사실을 알려야 한다.
 */
export async function setFileAssociation(enabled: boolean): Promise<void> {
  if (!isDesktopRuntime()) {
    throw new Error('데스크톱 앱에서만 파일 연결을 바꿀 수 있습니다.');
  }
  await invokeCommand<void>('set_file_association', { enabled });
}
