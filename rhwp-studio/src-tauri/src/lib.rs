mod atomic_save;
mod file_association;
mod file_open;

use tauri::Runtime;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

/// 진단 로그 플러그인.
///
/// **릴리스 빌드에서도 파일로 남긴다.** 사내 배포판에서 문제가 생기면 직원이 로그 파일 하나만
/// 보내면 원인을 좁힐 수 있어야 한다 — 예전에는 이 플러그인이 `debug_assertions` 로 감싸여
/// 있어 릴리스에는 로그가 아예 없었다.
///
/// 레벨을 `Info` 로 여는 것은 의도적이다. 세션 헤더와 문서 열기·저장 결과가 INFO 인데,
/// 여기서 `Warn` 으로 자르면 그것들이 기록 단계에서 통째로 버려진다. **볼륨 억제는 프론트엔드
/// 게이트에서 한다**(`src/core/diagnostics-log.ts`) — 무엇을 보낼지는 보내는 쪽이 정하고,
/// 여기서는 받은 것을 기록한다.
fn build_log_plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
  let mut targets = vec![Target::new(TargetKind::LogDir {
    file_name: Some("rhwp-studio".into()),
  })];
  if cfg!(debug_assertions) {
    targets.push(Target::new(TargetKind::Stdout));
  }

  tauri_plugin_log::Builder::default()
    .targets(targets)
    // 파일당 2MB × 3개 = 최대 약 6MB. 사내 배포에서 디스크를 잠식하지 않을 정도로 두되,
    // 재현까지 며칠 걸리는 문제도 뒤져볼 수 있게 한 세대는 남긴다.
    .rotation_strategy(RotationStrategy::KeepSome(3))
    .max_file_size(2 * 1024 * 1024)
    .timezone_strategy(TimezoneStrategy::UseLocal)
    .level(log::LevelFilter::Info)
    .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    // single-instance 는 다른 플러그인보다 먼저 등록해야 한다(플러그인 문서 요구사항).
    // 이미 창이 떠 있는데 탐색기에서 문서를 더블클릭하면 두 번째 인스턴스가 뜨는 대신
    // 그 argv 가 이쪽으로 전달된다 — 기존 창에서 문서를 연다.
    .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
      if let Some(path) = file_open::document_path_from_args(argv) {
        file_open::notify_open_path(app, path);
      }
    }))
    // 문서 저장은 fs 플러그인의 writeFile 이 아니라 atomic_save 커맨드를 쓴다 — 원본 경로에
    // 직접 쓰면 쓰기 도중 실패했을 때 원본이 잘린 채 남는다. 배경은 atomic_save 모듈 주석.
    .manage(atomic_save::SaveTokens::default())
    .manage(file_open::PendingOpen::default())
    .invoke_handler(tauri::generate_handler![
      atomic_save::begin_atomic_save,
      atomic_save::commit_atomic_save,
      file_open::take_pending_open_path,
      file_association::file_association_supported,
      file_association::file_association_enabled,
      file_association::set_file_association,
    ])
    // 데스크톱 셸은 File System Access API 대신 네이티브 다이얼로그로 파일을 고른다.
    // WebView2에서 FSA picker도 동작하기는 하지만(2026-08-09 실측), 경로 대신 핸들만
    // 주어 저장이 원본 경로 덮어쓰기로 확정되지 않고, 창 단위 "picker active" 플래그
    // 때문에 열기 요청이 겹치면 "File picker already active"로 실패한다.
    // 그래서 dialog 플러그인의 네이티브 다이얼로그로 파일을 고르고
    // fs 플러그인으로 경로를 직접 읽고 쓴다. dialog 플러그인이 선택된 경로를
    // fs 런타임 스코프에 등록하므로 별도 정적 스코프 설정은 두지 않는다.
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_fs::init())
    // 진단 로그 폴더를 탐색기로 여는 데 쓴다(도구 > 진단 로그 폴더 열기).
    .plugin(tauri_plugin_opener::init())
    .plugin(build_log_plugin())
    .setup(|app| {
      // 0.8.4 이하 설치 관리자가 등록한 옛 파일 연결을 새 방식으로 옮긴다.
      // 덮어 설치해도 옛 등록은 자동으로 사라지지 않아, 앱이 기동 시 한 번 정리한다.
      file_association::migrate_legacy_associations_on_startup();

      // 파일 연결로 실행된 경우의 문서 경로. 웹뷰가 아직 이벤트를 들을 수 없으므로
      // 대기열에 넣고, 프론트엔드가 준비된 뒤 take_pending_open_path 로 가져간다.
      let args: Vec<String> = std::env::args().collect();
      if let Some(path) = file_open::document_path_from_args(args) {
        file_open::remember_launch_path(app.handle(), path);
      }

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
