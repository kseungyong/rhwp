//! 파일 연결(.hwp/.hwpx)로 전달된 문서 경로를 프론트엔드로 넘긴다.
//!
//! 설치 파일이 확장자를 레지스트리에 등록하므로 탐색기 더블클릭은 앱을 `"<exe>" "<문서경로>"`
//! 형태로 실행한다. 셸이 그 인자를 소비하지 않으면 앱만 뜨고 문서는 열리지 않는다.
//!
//! 두 경로를 모두 다룬다.
//! - **최초 실행** — `setup` 에서 `std::env::args()` 를 읽어 대기열에 넣고, 웹뷰가 준비된 뒤
//!   `take_pending_open_path` 로 가져간다. 부팅 시점에는 프론트엔드가 아직 이벤트를 들을 수
//!   없으므로 이벤트가 아니라 대기열이어야 한다.
//! - **이미 실행 중** — single-instance 플러그인이 두 번째 인스턴스의 argv 를 넘겨주면
//!   이벤트로 보내고 기존 창을 앞으로 가져온다.
//!
//! 두 경우 모두 경로를 fs 런타임 스코프에 등록한다. 그러지 않으면 프론트엔드의 `readFile` 이
//! 권한 거부로 실패한다 — 다이얼로그로 고른 경로가 아니기 때문이다. OS 가 "이 문서를 열라"고
//! 전달한 경로이므로 사용자가 명시적으로 지목한 것과 같게 취급한다.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_fs::FsExt;

/// 실행 중인 창에 새 문서 경로를 알리는 이벤트 이름.
pub const OPEN_PATH_EVENT: &str = "rhwp://open-path";

/// 웹뷰가 준비되기 전에 도착한 실행 인자 경로.
#[derive(Default)]
pub struct PendingOpen(Mutex<Option<PathBuf>>);

fn has_document_extension(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("hwp") | Some("hwpx") | Some("hml")
    )
}

/// 실행 인자에서 열어야 할 문서 경로를 고른다.
///
/// `argv[0]`(실행 파일)은 건너뛰고, 옵션 플래그(`-`, `--`)가 아니면서 지원 확장자를 가진 첫
/// 인자를 고른다. WebView2 가 붙이는 진단용 플래그가 섞여 들어와도 문서로 오인하지 않는다.
pub fn document_path_from_args<I, S>(args: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .skip(1)
        .filter(|arg| !arg.as_ref().starts_with('-'))
        .map(|arg| PathBuf::from(arg.as_ref()))
        .find(|path| has_document_extension(path))
}

/// 경로를 fs 스코프에 등록한다. 등록에 실패하면 열기를 시도하지 않는다.
fn allow_path<R: Runtime>(app: &AppHandle<R>, path: &Path) -> bool {
    match app.try_fs_scope() {
        Some(scope) => scope.allow_file(path).is_ok(),
        None => false,
    }
}

/// 최초 실행 인자를 대기열에 넣는다.
pub fn remember_launch_path<R: Runtime>(app: &AppHandle<R>, path: PathBuf) {
    if !path.is_file() || !allow_path(app, &path) {
        return;
    }
    if let Ok(mut pending) = app.state::<PendingOpen>().0.lock() {
        *pending = Some(path);
    }
}

/// 이미 실행 중인 창에 새 경로를 알리고 창을 앞으로 가져온다.
pub fn notify_open_path<R: Runtime>(app: &AppHandle<R>, path: PathBuf) {
    if !path.is_file() || !allow_path(app, &path) {
        return;
    }
    let _ = app.emit(OPEN_PATH_EVENT, path.to_string_lossy().to_string());

    // 두 번째 인스턴스는 곧 종료되므로 사용자가 기존 창을 보게 만든다.
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// 대기 중인 실행 인자 경로를 가져간다(한 번만 반환된다).
#[tauri::command]
pub fn take_pending_open_path(pending: State<'_, PendingOpen>) -> Option<String> {
    pending
        .0
        .lock()
        .ok()
        .and_then(|mut slot| slot.take())
        .map(|path| path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::document_path_from_args;

    #[test]
    fn 실행파일_인자는_건너뛴다() {
        // argv[0] 이 .hwp 로 끝날 리는 없지만, 규칙이 위치가 아니라 확장자에만 기대지 않도록 확인한다.
        let args = vec!["app.exe", r"C:\문서\보고서.hwpx"];
        assert_eq!(
            document_path_from_args(args).unwrap().to_string_lossy(),
            r"C:\문서\보고서.hwpx"
        );
    }

    #[test]
    fn 지원_확장자만_고른다() {
        for ext in ["hwp", "hwpx", "hml", "HWP", "HwpX"] {
            let path = format!(r"C:\문서\문서.{ext}");
            let args = vec!["app.exe".to_string(), path.clone()];
            assert_eq!(
                document_path_from_args(args).unwrap().to_string_lossy(),
                path
            );
        }
    }

    #[test]
    fn 지원하지_않는_확장자는_무시한다() {
        let args = vec!["app.exe", r"C:\문서\사진.png", r"C:\문서\메모.txt"];
        assert!(document_path_from_args(args).is_none());
    }

    #[test]
    fn 옵션_플래그를_문서로_오인하지_않는다() {
        let args = vec!["app.exe", "--remote-debugging-port=9222", r"C:\문서\a.hwp"];
        assert_eq!(
            document_path_from_args(args).unwrap().to_string_lossy(),
            r"C:\문서\a.hwp"
        );
    }

    #[test]
    fn 인자가_없으면_없음이다() {
        assert!(document_path_from_args(vec!["app.exe"]).is_none());
        assert!(document_path_from_args(Vec::<String>::new()).is_none());
    }

    #[test]
    fn 여러_문서가_오면_첫_번째만_연다() {
        let args = vec!["app.exe", r"C:\a.hwp", r"C:\b.hwpx"];
        assert_eq!(
            document_path_from_args(args).unwrap().to_string_lossy(),
            r"C:\a.hwp"
        );
    }
}
