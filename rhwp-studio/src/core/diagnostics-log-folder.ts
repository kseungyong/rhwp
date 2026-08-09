/**
 * 진단 로그 폴더 열기 (데스크톱 전용).
 *
 * 로그가 남아도 사용자가 찾지 못하면 의미가 없다. 탐색기로 폴더를 열어 주고, 열기에 실패하면
 * 경로를 보여 주고 클립보드에 복사한다 — "눌렀는데 아무 일도 안 일어남"을 없애기 위한 폴백이다.
 * 클립보드도 실패할 수 있으므로 경로 문자열 자체는 항상 사용자에게 보인다.
 */

// node --test 에서 별칭(@/)이 해석되지 않으므로 상대 경로를 쓴다.
import { isDesktopRuntime } from '../command/desktop-file-system.ts';

export interface LogFolderResult {
  /** 탐색기로 여는 데 성공했는지. false 면 경로를 안내해야 한다. */
  revealed: boolean;
  /** 로그 폴더 경로. 안내에 쓴다. */
  path: string;
  /** 클립보드 복사 성공 여부. */
  copied: boolean;
}

/**
 * 로그 폴더를 연다.
 *
 * @throws 데스크톱이 아니거나 경로 자체를 알아낼 수 없을 때
 */
export async function openDiagnosticsLogFolder(): Promise<LogFolderResult> {
  if (!isDesktopRuntime()) {
    throw new Error('데스크톱 앱에서만 진단 로그를 남깁니다.');
  }

  const { appLogDir } = await import('@tauri-apps/api/path');
  const path = await appLogDir();

  let revealed = false;
  try {
    const { revealItemInDir } = await import('@tauri-apps/plugin-opener');
    await revealItemInDir(path);
    revealed = true;
  } catch {
    // 폴더가 아직 없거나 탐색기 호출이 막힌 경우 — 아래 폴백으로 넘어간다.
  }

  let copied = false;
  if (!revealed) {
    try {
      await navigator.clipboard.writeText(path);
      copied = true;
    } catch {
      // 클립보드 권한이 없을 수 있다. 경로는 호출부가 화면에 보여 준다.
    }
  }

  return { revealed, path, copied };
}
