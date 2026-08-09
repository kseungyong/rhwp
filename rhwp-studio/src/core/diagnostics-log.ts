/**
 * 진단 로그 퍼널 (데스크톱 전용).
 *
 * 목표는 하나다 — 사내 배포판에서 "안 돼요"라는 말을 들었을 때, 로그 파일 하나만 받아서 원인을
 * 좁힐 수 있을 것. 설계 근거는 `docs/superpowers/specs/2026-08-09-rhwp-studio-release-logging.md`.
 *
 * 세 가지를 한다.
 * 1. **전역 미처리 오류 수집** — `window.onerror` / `unhandledrejection`. 지금까지 이게 없어서
 *    `try/catch` 밖에서 터진 예외는 콘솔에만 남고 사용자에게는 "아무 반응 없음"으로 보였다.
 * 2. **게이트** — `console.error`/`warn` 과 명시적 {@link logLifecycle} 만 파일로 보낸다.
 *    `info`/`log`/`debug` 는 보내지 않는다. `console.warn` 이 152곳이라 전부 흘리면 신호가 묻힌다.
 * 3. **문서 내용 차단** — 기록하는 문자열은 300자에서 자르고, 인자는 원시값만 통과시킨다.
 *
 * 문서 편집기의 로그이므로 **본문 문자열은 어떤 경로로도 들어가면 안 된다.** 캡은 호출부가
 * 규칙을 어겼을 때를 대비한 2차 방어선이지 1차 방어가 아니다 — 애초에 문서에서 온 값을 로그에
 * 넘기지 않는 것이 원칙이다.
 */

// node --test 에서 별칭(@/)이 해석되지 않으므로 상대 경로를 쓴다.
import { isDesktopRuntime } from '../command/desktop-file-system.ts';

/** 한 조각의 최대 길이. 넘으면 자르고 원래 길이를 덧붙인다. */
export const MAX_FIELD_LENGTH = 300;

type LogLevel = 'info' | 'warn' | 'error';

export interface DiagnosticsSink {
  info(message: string): Promise<void> | void;
  warn(message: string): Promise<void> | void;
  error(message: string): Promise<void> | void;
}

/** 로그에 실을 수 있는 형태로 값을 줄인다. */
export function reduceArg(value: unknown): string {
  if (typeof value === 'string') return truncate(value);
  if (typeof value === 'number' || typeof value === 'boolean' || value == null) {
    return String(value);
  }
  if (value instanceof Error) {
    return `${value.name}: ${truncate(value.message)}`;
  }
  // 임의 객체를 직렬화하면 문서에서 온 값이 통째로 실릴 수 있다. 종류만 남긴다.
  const name = (value as object).constructor?.name ?? typeof value;
  return `[${name}]`;
}

export function truncate(text: string): string {
  if (text.length <= MAX_FIELD_LENGTH) return text;
  return `${text.slice(0, MAX_FIELD_LENGTH)}…(총 ${text.length}자)`;
}

/** 라이프사이클 이벤트 한 줄을 만든다. 값은 모두 축약을 거친다. */
export function formatLifecycle(
  event: string,
  fields: Record<string, unknown> = {},
): string {
  const parts = Object.entries(fields).map(([key, value]) => `${key}=${reduceArg(value)}`);
  return parts.length > 0 ? `[${event}] ${parts.join(' ')}` : `[${event}]`;
}

let sink: DiagnosticsSink | null = null;
/** 전송 도중 발생한 오류가 다시 퍼널을 타지 않게 하는 빗장. */
let isForwarding = false;

function forward(level: LogLevel, message: string): void {
  if (!sink || isForwarding) return;
  isForwarding = true;
  try {
    void Promise.resolve(sink[level](message)).catch(() => {
      // 전송 실패는 버린다. 여기서 console.error 를 부르면 무한 루프가 된다.
    });
  } catch {
    // 로그 때문에 앱이 죽어서는 안 된다.
  } finally {
    isForwarding = false;
  }
}

/**
 * 명시적 라이프사이클 이벤트를 남긴다(INFO).
 *
 * 일반 `console.info` 는 파일로 가지 않으므로, 무엇이 기록되는지가 호출부에서 드러난다.
 * **파일명·본문 등 문서에서 온 값은 넣지 않는다** — 형식·크기·페이지 수처럼 구조 정보만 넣는다.
 */
export function logLifecycle(event: string, fields: Record<string, unknown> = {}): void {
  forward('info', formatLifecycle(event, fields));
}

/**
 * 퍼널을 설치한다. 웹에서는 아무 것도 하지 않는다.
 *
 * @param injectedSink 테스트용 대체 sink. 없으면 Tauri 로그 플러그인을 지연 로드한다.
 * @returns 설치 여부
 */
export async function installDiagnosticsLog(
  injectedSink?: DiagnosticsSink,
): Promise<boolean> {
  if (!injectedSink && !isDesktopRuntime()) return false;

  try {
    sink = injectedSink ?? (await import('@tauri-apps/plugin-log'));
  } catch {
    return false;
  }

  // 원본을 캡처해 둔다. 전송 경로 안에서는 이 원본만 쓴다.
  const original = {
    error: console.error.bind(console),
    warn: console.warn.bind(console),
  };

  console.error = (...args: unknown[]) => {
    original.error(...args);
    forward('error', args.map(reduceArg).join(' '));
  };
  console.warn = (...args: unknown[]) => {
    original.warn(...args);
    forward('warn', args.map(reduceArg).join(' '));
  };

  if (typeof window !== 'undefined') {
    window.addEventListener('error', (event) => {
      const where = `${event.filename ?? '?'}:${event.lineno ?? 0}`;
      forward('error', `[uncaught] ${reduceArg(event.message)} @ ${truncate(where)}`);
    });
    window.addEventListener('unhandledrejection', (event) => {
      forward('error', `[unhandled-rejection] ${reduceArg(event.reason)}`);
    });
  }

  return true;
}

/** 테스트에서 퍼널 상태를 되돌린다. */
export function resetDiagnosticsLogForTest(): void {
  sink = null;
  isForwarding = false;
}
