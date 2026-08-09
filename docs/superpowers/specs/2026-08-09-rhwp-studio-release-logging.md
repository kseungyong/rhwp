# RHWP Studio 릴리스 진단 로그 설계

## 배경과 목적

사내 배포판(RHWP Studio)에서 문제가 생기면 현재 확인할 수단이 없다. 세 가지가 겹쳐 있다.

- `src-tauri/src/lib.rs`의 로그 플러그인이 `if cfg!(debug_assertions)`로 감싸여 있어 **릴리스
  빌드에는 로그 파일이 없다.**
- 프론트엔드에 **전역 미처리 오류 핸들러가 없다** — `window.onerror`도 `unhandledrejection`도
  등록하지 않는다. `try/catch` 밖에서 터진 예외는 콘솔에만 남고 사용자에게는 "아무 반응 없음"으로
  보인다. 데스크톱에서는 콘솔을 열 방법도 마땅치 않다.
- 원격 리포팅이 없다(의도된 설계 — 아래 [범위 밖](#범위-밖) 참고).

목표는 하나다. **직원이 "안 돼요"라고 할 때 로그 파일 하나만 받으면 원인을 좁힐 수 있을 것.**
재현 요청 없이 1차 판단이 되는 것이 성공 기준이다.

대상은 데스크톱 빌드뿐이다. 웹 빌드는 브라우저 콘솔·개발자 도구가 있으므로 동작을 바꾸지 않는다.

## 확정된 요구사항

- 릴리스 빌드에서 로그가 파일로 남는다.
- `try/catch` 밖의 미처리 예외와 거부된 Promise가 빠짐없이 기록된다.
- **문서 내용은 어떤 경로로도 로그에 들어가지 않는다.**
- 사용자가 로그 파일을 스스로 찾아 첨부할 수 있다.
- 로그 기능의 실패가 앱 동작에 영향을 주지 않는다.
- 웹 빌드에는 Tauri 코드가 섞이지 않는다.

## 1. 남길 것과 남기지 않을 것

문서 편집기의 로그라 이 결정이 가장 먼저다. **문서에서 온 문자열은 로그에 넣지 않는다.**

| 남긴다 | 남기지 않는다 |
|---|---|
| 형식(hwp/hwpx/hml), 바이트 크기, 페이지 수, 구역 수 | 본문 텍스트, 셀 값, 누름틀 값, 책갈피 이름 |
| 파서·렌더러 구조 오류(`미등록 ID 참조: styleIDRef`) | 파일 전체 경로 |
| 커맨드 ID, 예외 이름·스택 | 사용자 입력 문자열 |

부득이 문서 파생 값을 남겨야 하면 길이와 SHA-256 앞 8자로 대체한다. 저장소에 이미
`untrustedContent` 출처 표지 개념([`envelope_provenance.md`](../../../mydocs/tech/envelope_provenance.md))이
있으므로 그 경계를 그대로 쓴다.

### 예외 메시지를 통한 유입 방어

파서·로직이 `throw new Error(\`... ${본문조각}\`)` 형태로 **예외 메시지 자체에 문서 조각을 담는**
경우가 있다. 정책만으로는 막히지 않으므로 퍼널에 기계적 방어선을 둔다.

- 기록하는 모든 문자열을 **300자에서 자른다**(잘린 경우 `…(총 N자)` 표기).
- 전송 인자는 문자열·숫자·불리언만 통과시키고, 객체는 생성자 이름만 남긴다. 임의 객체를
  통째로 직렬화하지 않는다.

캡은 정보 누출의 상한을 보장할 뿐 완전한 차단은 아니다. 근본 방어는 위의 "남기지 않는다" 규칙을
호출부에서 지키는 것이고, 캡은 그것이 뚫렸을 때의 2차 방어선이다.

## 2. 구조

### (a) Rust — 로그 타깃

`src-tauri/src/lib.rs`의 `debug_assertions` 조건을 걷어내고 릴리스에서도 붙인다.

```rust
tauri_plugin_log::Builder::default()
    .target(Target::new(TargetKind::LogDir { file_name: Some("rhwp-studio".into()) }))
    .rotation_strategy(RotationStrategy::KeepSome(3))
    .max_file_size(2 * 1024 * 1024)   // 파일당 2MB → 최대 약 6MB
    .level(LevelFilter::Info)
    .timezone_strategy(TimezoneStrategy::UseLocal)
```

디버그 빌드에는 `TargetKind::Stdout`을 추가로 붙여 개발 경험을 유지한다.

저장 위치는 플러그인의 `LogDir` 규약을 따른다 — Windows에서
`%LOCALAPPDATA%\com.rhwp.studio\logs\`.

**레벨을 `Info`로 여는 이유.** 파일 타깃 레벨을 `Warn`으로 걸면 아래 (b)에서 정의하는 세션 헤더와
문서 열기·저장 결과가 **파일 기록 단계에서 드롭된다.** 볼륨 억제는 Rust 타깃 레벨이 아니라
프론트엔드 게이트에서 한다 — 무엇을 보낼지는 보내는 쪽이 결정하고, Rust는 받은 것을 기록한다.

### (b) 프론트엔드 — 퍼널과 게이트

신규 `rhwp-studio/src/core/diagnostics-log.ts`.

**전역 핸들러** — 지금 가장 큰 구멍이다.

- `window.addEventListener('error')` → ERROR
- `window.addEventListener('unhandledrejection')` → ERROR

**게이트** — 볼륨 억제는 여기서 한다. 현재 `console.warn`이 152곳인데 대부분 정상 폴백 경로다.
전부 파일로 보내면 신호가 묻힌다.

| 대상 | 전송 |
|---|---|
| `console.error` / `console.warn` | 보낸다 |
| `console.info` / `console.debug` / `console.log` | **보내지 않는다** |
| 명시적 라이프사이클 이벤트 API | 보낸다 (INFO) |

라이프사이클 이벤트는 `logLifecycle(event, fields)` 같은 명시적 호출로만 남긴다. 일반 `console.info`를
타지 않으므로 무엇이 기록되는지가 호출부에서 드러난다. 최소 두 가지를 남긴다.

- **세션 헤더**(부팅 직후 1회) — 앱 버전, WebView2/UA 버전, OS, 렌더 백엔드와 폴백 사유.
  이 한 줄이 진단의 절반이다.
- **문서 열기·저장 결과** — 형식, 바이트 크기, 페이지 수, 소요 ms. 파일명은 넣지 않는다.

**재귀 차단.** 로그 전송이 실패해 `console.error`를 부르면 무한 루프가 된다.

- 퍼널 초기화 시 원본 함수를 캡처한다(`const originalError = console.error.bind(console)`).
- 전송 중 플래그(`let isForwarding = false`)를 두고, 전송 경로 안에서는 원본 console만 쓴다.

**실패 격리.** 전 구간을 `try/catch`로 감싸고 로깅 실패는 조용히 버린다. 로그 때문에 앱이 죽는 일은
없어야 한다.

**웹 무영향.** `isDesktopRuntime()`(`command/desktop-file-system.ts`에 이미 있음)으로 분기하고
`@tauri-apps/plugin-log`은 동적 import 한다. 웹 빌드에서는 초기화 자체가 no-op이고 플러그인 청크는
로드되지 않는다.

### (c) 사용자 접근 경로

로그가 있어도 찾지 못하면 의미가 없다. `도구` 메뉴에 **"진단 로그 폴더 열기"** 커맨드를 둔다.

1. 기본 — `tauri-plugin-opener`의 `revealItemInDir`로 탐색기에서 폴더를 연다.
2. 폴백 — 열기에 실패하거나 데스크톱 런타임이 아니면 **경로를 대화상자로 표시하고
   `navigator.clipboard`로 복사**한다.

폴백은 "실패 시 아무 일도 일어나지 않는" 상태를 없애기 위한 것이다. 클립보드 API도 실패할 수 있으므로
경로 텍스트 자체는 대화상자에 항상 보이게 둔다.

직원용 설치 안내에는 "문제가 생기면 이 폴더의 최신 파일을 첨부해 주세요" 한 줄과 경로를 넣는다.

## 3. 변경 지점

```
src-tauri/Cargo.toml                   tauri-plugin-opener 추가
src-tauri/src/lib.rs                   로그 타깃 설정, debug 조건 제거
src-tauri/capabilities/default.json    log:default, opener 권한
rhwp-studio/package.json               @tauri-apps/plugin-log, @tauri-apps/plugin-opener
src/core/diagnostics-log.ts            신규 — 퍼널·게이트·전역 핸들러
src/main.ts                            부팅 최초에 초기화 + 세션 헤더 기록
src/command/commands/tool.ts           "진단 로그 폴더 열기" 커맨드
docs/windows-packaging/EMPLOYEE_INSTALL_GUIDE.md   로그 첨부 안내
```

## 4. 테스트 계획

단위 테스트는 퍼널의 순수 로직에 건다.

- 재귀 차단 — 전송 경로에서 발생한 오류가 다시 퍼널을 타지 않는다.
- 게이트 — `console.info`/`log`/`debug`는 전송되지 않고, `warn`/`error`와 명시적 라이프사이클
  이벤트만 전송된다.
- 300자 캡과 잘림 표기, 객체 인자의 생성자 이름 축약.
- 웹 런타임에서 초기화가 no-op이다.

나머지는 실측한다.

- 의도적 오류(전역 미처리 예외, 거부된 Promise)를 던져 파일에 남는지.
- 2MB를 넘겨 회전이 도는지, 최대 3개가 유지되는지.
- 웹 빌드 산출물에 Tauri 플러그인 코드가 섞이지 않는지.
- **문서를 열고 편집·저장한 뒤 로그에 본문 문자열이 한 조각도 없는지.** 이 항목이 이 설계의 핵심
  게이트다 — 실패하면 나머지가 다 통과해도 배포하지 않는다.

## 범위 밖

- **원격 전송(Sentry 등).** 오프라인 우선 설계에 어긋나고, 문서 내용이 외부로 나갈 위험을 새로
  만든다. 로그는 로컬에만 두고 사용자가 직접 첨부한다.
- 로그 뷰어 UI. 텍스트 파일과 폴더 열기로 충분하다.
- 웹 빌드의 로그 수집.

## 의존 관계

이 작업은 데스크톱 셸(`rhwp-studio/src-tauri/`)을 전제한다. 해당 셸은
[PR #4322](https://github.com/edwardkim/rhwp/pull/4322)에서 처음 들어가므로, 이 변경의 PR은 #4322
병합 이후에 연다. #4322 검토 과정에서 `lib.rs`나 `capabilities/default.json`이 바뀌면 이 브랜치도
맞춰 조정한다.
