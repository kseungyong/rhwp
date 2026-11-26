use std::sync::Mutex;

use tauri::Manager;
use tauri_plugin_fs::FsExt;

/// 파일 연결(더블클릭)로 전달된 문서 경로 큐.
///
/// - Windows/Linux: OS가 경로를 프로세스 인자(argv)로 넘기므로 시작 시 한 번 수집한다.
/// - macOS: LaunchServices가 Apple Event(`RunEvent::Opened`)로 넘긴다. WebView가 뜨기
///   전에 도착할 수 있으므로(첫 더블클릭 실행) 프런트엔드가 가져갈 때까지 버퍼링한다.
///
/// 프런트엔드(desktop-launch-file.ts)는 초기화가 끝나면 `take_launch_files`로 큐를
/// 비우고, 그 뒤에 도착하는 열기 요청은 `desktop:launch-files` 이벤트로 바로 받는다.
#[derive(Default)]
struct LaunchFiles {
  pending: Vec<String>,
  /// 프런트엔드가 take_launch_files를 호출했는지 — 이후에는 버퍼 대신 이벤트로 보낸다.
  frontend_ready: bool,
}

/// 전달받은 경로를 fs 플러그인 런타임 스코프에 등록하고 문자열로 돌려준다.
/// 다이얼로그로 고른 파일과 같은 권한을 줘야 열기 뒤 "같은 경로에 저장"이 동작한다.
fn admit_launch_path(app: &tauri::AppHandle, path: &std::path::Path) -> Option<String> {
  if !path.is_file() {
    return None;
  }
  if let Err(error) = app.fs_scope().allow_file(path) {
    eprintln!("launch file fs scope 등록 실패 ({}): {error}", path.display());
    return None;
  }
  Some(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn take_launch_files(state: tauri::State<'_, Mutex<LaunchFiles>>) -> Vec<String> {
  let mut launch = state.lock().expect("launch files state poisoned");
  launch.frontend_ready = true;
  std::mem::take(&mut launch.pending)
}

/// macOS: 실행 중 파일 연결 열기(Apple Event). 프런트엔드가 준비됐으면 이벤트로,
/// 아직이면(첫 실행 직후) 버퍼에 쌓아 `take_launch_files`가 가져가게 한다.
#[cfg(target_os = "macos")]
fn handle_opened_urls(app: &tauri::AppHandle, urls: Vec<tauri::Url>) {
  use tauri::Emitter;

  let paths: Vec<String> = urls
    .iter()
    .filter_map(|url| url.to_file_path().ok())
    .filter_map(|path| admit_launch_path(app, &path))
    .collect();
  if paths.is_empty() {
    return;
  }
  let state = app.state::<Mutex<LaunchFiles>>();
  let mut launch = state.lock().expect("launch files state poisoned");
  if launch.frontend_ready {
    if let Err(error) = app.emit("desktop:launch-files", &paths) {
      eprintln!("launch file 이벤트 발송 실패: {error}");
    }
  } else {
    launch.pending.extend(paths);
  }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let app = tauri::Builder::default()
    // 데스크톱 셸은 File System Access API 대신 네이티브 다이얼로그로 파일을 고른다.
    // WebView2에서 FSA picker도 동작하기는 하지만(2026-08-09 실측), 경로 대신 핸들만
    // 주어 저장이 원본 경로 덮어쓰기로 확정되지 않고, 창 단위 "picker active" 플래그
    // 때문에 열기 요청이 겹치면 "File picker already active"로 실패한다.
    // 그래서 dialog 플러그인의 네이티브 다이얼로그로 파일을 고르고
    // fs 플러그인으로 경로를 직접 읽고 쓴다. dialog 플러그인이 선택된 경로를
    // fs 런타임 스코프에 등록하므로 별도 정적 스코프 설정은 두지 않는다.
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_fs::init())
    // 자동 업데이트 — 앱 시작 시 프런트엔드(desktop-updater.ts)가 GitHub Releases의
    // latest.json을 확인해 새 버전을 내려받아 설치한다. process 플러그인은 설치 후
    // 재시작(relaunch)에 쓴다.
    .plugin(tauri_plugin_process::init())
    // 파일 연결 열기 큐 — macOS는 첫 실행의 odoc Apple Event(RunEvent::Opened)가
    // setup()보다 먼저 도착할 수 있으므로 Builder 단계에서 등록해야 한다(늦으면
    // 이벤트 핸들러의 state() 조회가 panic → Apple Event 디스패치 중 abort).
    .manage(Mutex::new(LaunchFiles::default()))
    .invoke_handler(tauri::generate_handler![take_launch_files])
    .setup(|app| {
      #[cfg(desktop)]
      app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      // Windows/Linux: 파일 연결로 실행되면 문서 경로가 argv로 온다.
      // macOS는 Apple Event(RunEvent::Opened)로 오므로 argv를 보지 않는다.
      #[cfg(not(target_os = "macos"))]
      {
        let handle = app.handle();
        let pending: Vec<String> = std::env::args_os()
          .skip(1)
          .filter_map(|arg| admit_launch_path(handle, std::path::Path::new(&arg)))
          .collect();
        if !pending.is_empty() {
          app
            .state::<Mutex<LaunchFiles>>()
            .lock()
            .expect("launch files state poisoned")
            .pending = pending;
        }
      }
      Ok(())
    })
    .build(tauri::generate_context!())
    .expect("error while running tauri application");

  app.run(|app_handle, event| match event {
    #[cfg(target_os = "macos")]
    tauri::RunEvent::Opened { urls } => handle_opened_urls(app_handle, urls),
    _ => {
      let _ = app_handle;
    }
  });
}
