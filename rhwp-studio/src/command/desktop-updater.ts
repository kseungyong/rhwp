/**
 * 데스크톱 셸 자동 업데이트
 *
 * 포털(portal.kdsys.co.kr)의 공개 다운로드 경로에서 latest.json을 확인해 새 버전을
 * 내려받아 설치한다. 업데이트 패키지의 진위는 Tauri updater가 tauri.conf.json의
 * pubkey(minisign)로 검증하므로, 전송 경로가 공개여도 위변조된 패키지는 설치되지 않는다.
 *
 * - 앱 시작 시: 조용히 확인하고, 새 버전이 있을 때만 사용자에게 설치를 묻는다.
 * - "업데이트 확인" 메뉴: 결과(최신/새 버전/실패)를 항상 토스트나 대화상자로 알린다.
 *
 * Tauri 플러그인 JS는 동적 import로만 불러온다 — 웹(PWA) 빌드 번들에 섞이지 않고,
 * 데스크톱 런타임에서만 별도 청크로 로드된다.
 */
import { isDesktopRuntime } from './desktop-file-system';
import { showConfirm } from '@/ui/confirm-dialog';
import { showToast } from '@/ui/toast';
import { t } from '@/i18n/index.ts';

/** 중복 실행 방지 — 시작 확인과 메뉴 확인이 겹쳐도 다운로드는 한 번만 한다. */
let updateInFlight = false;

/**
 * 업데이트를 확인하고, 새 버전이 있으면 사용자 동의 후 내려받아 설치한다.
 *
 * @param silent true면 시작 시 자동 확인 — 새 버전이 없거나 확인에 실패해도 조용히 넘어간다.
 *               false면 메뉴에서 수동 확인 — 결과를 항상 알린다.
 */
export async function checkForUpdates(silent: boolean): Promise<void> {
  if (!isDesktopRuntime() || updateInFlight) return;
  updateInFlight = true;
  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    const update = await check();
    if (!update) {
      if (!silent) showToast({ message: t('updater.upToDate') });
      return;
    }

    const ok = await showConfirm(
      t('updater.availableTitle'),
      t('updater.availableMessage', { p1: update.version }),
    );
    if (!ok) return;

    showToast({ message: t('updater.downloading'), durationMs: 0 });
    await update.downloadAndInstall();

    // Windows(NSIS)는 설치 프로그램이 앱을 종료·재시작하므로 여기 도달하지 않을 수 있다.
    // 다른 플랫폼이나 설치가 재시작 없이 끝난 경우만 직접 재시작한다.
    const { relaunch } = await import('@tauri-apps/plugin-process');
    await relaunch();
  } catch (err) {
    // 포털에 닿지 못하는 사내망 밖 환경 등 — 시작 확인은 조용히 포기한다.
    console.warn('[updater] 업데이트 확인 실패:', err);
    if (!silent) {
      showToast({ message: t('updater.checkFailed'), confirmLabel: t('updater.close'), durationMs: 0 });
    }
  } finally {
    updateInFlight = false;
  }
}

/** 앱 시작 시 1회 조용히 확인한다. 문서 로드 등 주 경로를 막지 않도록 fire-and-forget. */
export function scheduleStartupUpdateCheck(): void {
  if (!isDesktopRuntime()) return;
  // 초기 렌더·WASM 로드와 경합하지 않게 잠시 뒤로 미룬다.
  setTimeout(() => { void checkForUpdates(true); }, 3000);
}
