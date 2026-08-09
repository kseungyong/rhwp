mod atomic_save;
mod file_association;
mod file_open;

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
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }

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
