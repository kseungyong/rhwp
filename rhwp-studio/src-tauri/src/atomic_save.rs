//! 문서 저장을 원자적으로 수행하는 커맨드.
//!
//! 원본 경로에 곧바로 쓰면 쓰기 도중 실패(디스크 부족, 파일 잠김, 전원 차단)했을 때
//! 원본이 잘린 채 남는다. 같은 디렉터리에 임시 파일을 만들어 기록·flush 한 뒤 교체한다.
//! 실패하면 임시 파일만 지우고 원본은 손대지 않는다.
//!
//! fs 플러그인이 아니라 전용 커맨드를 두는 이유는 스코프 때문이다. dialog 플러그인은
//! 사용자가 고른 **파일 경로 하나만** fs 런타임 스코프에 등록하므로(`allow_file`),
//! 형제 임시 파일에 대한 `writeFile` 은 권한 거부로 실패한다. 정적 스코프를 넓히는 대신
//! 여기서 직접 처리하되, 권한 판단은 같은 fs 스코프를 그대로 물어본다 — 사용자가 이
//! 세션에서 다이얼로그로 고른 경로만 저장 대상이 된다.
//!
//! 바이트는 raw IPC body 로 받는다. 커맨드 인자(JSON)로 `Vec<u8>` 를 넘기면 5MB 문서가
//! 20MB 남짓한 JSON 문자열이 되어 저장이 눈에 띄게 느려진다. 경로는 헤더에 담을 수 없어
//! (Windows 경로에 비ASCII 문자가 흔하다) `begin` 에서 토큰을 발급받아 그 토큰만 헤더로
//! 보낸다.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Runtime, State};
use tauri_plugin_fs::FsExt;

/// 발급된 저장 토큰 → 대상 경로.
#[derive(Default)]
pub struct SaveTokens(Mutex<SaveTokenState>);

#[derive(Default)]
struct SaveTokenState {
    next_id: u64,
    pending: HashMap<u64, PathBuf>,
}

/// 저장 대상 경로를 등록하고 토큰을 발급한다.
///
/// 경로 검증은 여기서 한 번만 한다 — 뒤이은 `commit_atomic_save` 는 토큰이 가리키는
/// 경로만 쓰므로, 프론트엔드가 임의 경로를 밀어 넣을 수 없다.
#[tauri::command]
pub fn begin_atomic_save<R: Runtime>(
    app: AppHandle<R>,
    tokens: State<'_, SaveTokens>,
    path: String,
) -> Result<String, String> {
    let target = PathBuf::from(&path);

    if !app.fs_scope().is_allowed(&target) {
        return Err("저장이 허용되지 않은 경로입니다. 파일을 다시 선택해 주세요.".into());
    }
    if target.parent().is_none() {
        return Err("저장 대상 폴더를 확인할 수 없습니다.".into());
    }

    let mut state = tokens.0.lock().map_err(|_| "저장 상태에 접근하지 못했습니다.")?;
    state.next_id += 1;
    let id = state.next_id;
    state.pending.insert(id, target);
    Ok(id.to_string())
}

/// 임시 파일에 기록·flush 한 뒤 대상 경로로 교체한다.
///
/// 토큰은 성공·실패와 무관하게 소비된다(재사용 불가).
#[tauri::command]
pub fn commit_atomic_save(
    tokens: State<'_, SaveTokens>,
    request: Request<'_>,
) -> Result<(), String> {
    let token: u64 = request
        .headers()
        .get("x-rhwp-save-token")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .ok_or("저장 토큰이 없습니다.")?;

    let target = {
        let mut state = tokens.0.lock().map_err(|_| "저장 상태에 접근하지 못했습니다.")?;
        state.pending.remove(&token).ok_or("만료된 저장 토큰입니다.")?
    };

    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("저장할 데이터가 올바르지 않습니다.".into());
    };

    write_atomically(&target, bytes).map_err(|e| e.to_string())
}

/// 같은 디렉터리의 임시 파일에 쓰고 flush 한 뒤 교체한다.
///
/// 임시 파일을 **같은 디렉터리**에 두는 것이 핵심이다. 다른 볼륨(예: %TEMP%)에 만들면
/// 교체가 원자적 rename 이 아니라 복사 후 삭제로 처리되어, 목적 자체가 무너진다.
fn write_atomically(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = target.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "대상 폴더를 확인할 수 없습니다")
    })?;
    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document");
    let temp_path = dir.join(format!(".{file_name}.rhwp-save"));

    // 이전 실행이 남긴 임시 파일이 있으면 덮어쓴다.
    let write_result = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp_path)?;
        file.write_all(bytes)?;
        // 메타데이터까지 디스크에 내린다 — 교체 직후 전원이 끊겨도 내용이 남도록.
        file.sync_all()?;
        Ok(())
    })();

    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&temp_path);
        return Err(e);
    }

    // 같은 볼륨이므로 rename 이 기존 파일을 원자적으로 대체한다.
    if let Err(e) = std::fs::rename(&temp_path, target) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_atomically;

    #[test]
    fn 교체_후_내용이_새_바이트다() {
        let dir = std::env::temp_dir().join("rhwp-atomic-save-1");
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join("문서.hwpx");
        std::fs::write(&target, b"old").unwrap();

        write_atomically(&target, b"new-content").unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"new-content");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 임시_파일을_남기지_않는다() {
        let dir = std::env::temp_dir().join("rhwp-atomic-save-2");
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join("문서.hwpx");

        write_atomically(&target, b"data").unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|name| name != "문서.hwpx")
            .collect();
        assert!(leftovers.is_empty(), "임시 파일이 남았다: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 대상이_없어도_새로_만든다() {
        let dir = std::env::temp_dir().join("rhwp-atomic-save-3");
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join("새문서.hwp");
        let _ = std::fs::remove_file(&target);

        write_atomically(&target, b"fresh").unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"fresh");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 쓰기_실패시_원본을_건드리지_않는다() {
        let dir = std::env::temp_dir().join("rhwp-atomic-save-4");
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join("원본.hwpx");
        std::fs::write(&target, b"original").unwrap();

        // 임시 파일 경로를 디렉터리로 선점해 File::create 를 실패시킨다.
        let blocked = dir.join(".원본.hwpx.rhwp-save");
        std::fs::create_dir_all(&blocked).unwrap();

        assert!(write_atomically(&target, b"replacement").is_err());
        // 원본은 그대로여야 한다 — 이 보장이 이 모듈의 존재 이유다.
        assert_eq!(std::fs::read(&target).unwrap(), b"original");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
