//! `.hwp`/`.hwpx` 파일 연결을 앱 안에서 켜고 끈다.
//!
//! 설치 관리자가 연결을 무조건 등록하지 않는다(`tauri.conf.json` 의 `fileAssociations` 를
//! 두지 않는 이유). 한컴 제품이 이미 깔린 PC 에서 설치만으로 연결이 바뀌면 안 되고, 설계
//! 스펙도 "선택하게 하고 강제 덮어쓰기는 하지 않는다"로 정했다. 선택 시점을 설치 관리자에
//! 두려면 NSIS 템플릿을 포크해야 해서, 앱의 환경 설정으로 옮겼다.
//!
//! 쓰기 대상은 `HKCU\Software\Classes` 다. 관리자 권한이 필요 없고, 이 사용자에게만 적용되며,
//! 시스템 전역(HKLM)에 등록된 다른 프로그램의 연결을 건드리지 않는다.
//!
//! 끄면 이전 연결을 되돌린다. 켤 때 기존 기본값을 `_backup` 값으로 남겨 두고, 끌 때 그 값을
//! 복원한다 — 한컴 연결을 덮어썼다면 해제 시 한컴으로 돌아간다.

#[cfg(windows)]
mod windows_impl {
    use std::path::{Path, PathBuf};

    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    /// 우리가 소유하는 ProgID. 다른 프로그램과 충돌하지 않게 제품 고유 이름을 쓴다.
    const PROG_IDS: [(&str, &str, &str); 2] = [
        ("hwp", "RhwpStudio.hwp", "아래한글 HWP 문서"),
        ("hwpx", "RhwpStudio.hwpx", "아래한글 HWPX 문서"),
    ];

    const BACKUP_VALUE: &str = "RhwpStudio_backup";

    fn classes() -> std::io::Result<RegKey> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags("Software\\Classes", KEY_READ | KEY_WRITE)
    }

    /// 확장자의 현재 기본 ProgID (없으면 빈 문자열).
    fn current_prog_id(classes: &RegKey, ext: &str) -> String {
        classes
            .open_subkey(format!(".{ext}"))
            .and_then(|k| k.get_value::<String, _>(""))
            .unwrap_or_default()
    }

    /// 두 확장자가 모두 우리 ProgID 를 가리키면 켜진 것으로 본다.
    pub fn is_enabled() -> Result<bool, String> {
        let classes = classes().map_err(|e| e.to_string())?;
        Ok(PROG_IDS
            .iter()
            .all(|(ext, prog_id, _)| current_prog_id(&classes, ext) == *prog_id))
    }

    pub fn set_enabled(enabled: bool, exe: PathBuf) -> Result<(), String> {
        let classes = classes().map_err(|e| e.to_string())?;
        for (ext, prog_id, description) in PROG_IDS {
            if enabled {
                register(&classes, ext, prog_id, description, &exe)
            } else {
                unregister(&classes, ext, prog_id)
            }
            .map_err(|e| format!(".{ext} 연결을 바꾸지 못했습니다: {e}"))?;
        }
        notify_shell();
        Ok(())
    }

    fn register(
        classes: &RegKey,
        ext: &str,
        prog_id: &str,
        description: &str,
        exe: &Path,
    ) -> std::io::Result<()> {
        let exe = exe.to_string_lossy().to_string();

        let (ext_key, _) = classes.create_subkey(format!(".{ext}"))?;
        // 이미 우리 것이면 백업을 덮어쓰지 않는다 — 두 번 켜면 원래 값을 잃는다.
        let previous = ext_key.get_value::<String, _>("").unwrap_or_default();
        if previous != prog_id {
            ext_key.set_value(BACKUP_VALUE, &previous)?;
        }
        ext_key.set_value("", &prog_id)?;

        let (class_key, _) = classes.create_subkey(prog_id)?;
        class_key.set_value("", &description)?;
        class_key
            .create_subkey("DefaultIcon")?
            .0
            .set_value("", &format!("{exe},0"))?;
        class_key
            .create_subkey("shell\\open\\command")?
            .0
            .set_value("", &format!("\"{exe}\" \"%1\""))?;
        Ok(())
    }

    fn unregister(classes: &RegKey, ext: &str, prog_id: &str) -> std::io::Result<()> {
        if let Ok(ext_key) = classes.open_subkey_with_flags(format!(".{ext}"), KEY_READ | KEY_WRITE)
        {
            // 우리가 잡고 있을 때만 되돌린다. 그 사이 다른 프로그램이 가져갔다면 건드리지 않는다.
            if ext_key.get_value::<String, _>("").unwrap_or_default() == prog_id {
                let backup = ext_key.get_value::<String, _>(BACKUP_VALUE).unwrap_or_default();
                ext_key.set_value("", &backup)?;
            }
            let _ = ext_key.delete_value(BACKUP_VALUE);
        }
        let _ = classes.delete_subkey_all(prog_id);
        Ok(())
    }

    /// 탐색기에 연결 변경을 알린다. 없으면 아이콘·기본 프로그램이 한동안 갱신되지 않는다.
    fn notify_shell() {
        const SHCNE_ASSOCCHANGED: u32 = 0x0800_0000;
        const SHCNF_IDLIST: u32 = 0x0000;
        unsafe {
            windows_sys::Win32::UI::Shell::SHChangeNotify(
                SHCNE_ASSOCCHANGED as i32,
                SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            );
        }
    }
}

/// 이 플랫폼에서 파일 연결 전환을 지원하는지.
#[tauri::command]
pub fn file_association_supported() -> bool {
    cfg!(windows)
}

/// 현재 `.hwp`/`.hwpx` 가 이 앱으로 연결돼 있는지.
#[tauri::command]
pub fn file_association_enabled() -> Result<bool, String> {
    #[cfg(windows)]
    {
        windows_impl::is_enabled()
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

/// 파일 연결을 켜거나 끈다.
#[tauri::command]
pub fn set_file_association(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        windows_impl::set_enabled(enabled, exe)
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("이 플랫폼에서는 파일 연결을 바꿀 수 없습니다.".into())
    }
}
