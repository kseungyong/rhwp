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

    /// 0.8.4 이하의 설치 관리자(NSIS `APP_ASSOCIATE`)가 등록하던 ProgID.
    /// `(확장자, 옛 ProgID, 옛 백업 값 이름)`.
    const LEGACY_PROG_IDS: [(&str, &str, &str); 2] = [
        ("hwp", "HWP Document", "HWP Document_backup"),
        ("hwpx", "HWPX Document", "HWPX Document_backup"),
    ];

    /// 옛 ProgID 가 우리 것인지 판별하는 표지. 설치 경로는
    /// `%LOCALAPPDATA%\RHWP Studio\app.exe` 였다.
    const LEGACY_COMMAND_MARKER: &str = r"\rhwp studio\app.exe";

    /// 옛 등록 처리 결과. 테스트가 분기별로 확인한다.
    #[derive(Debug, PartialEq, Eq)]
    pub enum LegacyOutcome {
        /// 옛 등록이 없다.
        NotFound,
        /// 이름은 같지만 다른 프로그램의 등록이다 — 건드리지 않았다.
        NotOurs,
        /// 새 방식으로 옮겼다(기본값 승계 포함).
        Migrated,
        /// 기본값은 이미 남이 가져갔다 — 옛 키만 정리했다.
        RemovedOnly,
    }

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

    /// 옛 등록(설치 관리자가 만든 것) 하나를 새 방식으로 옮긴다.
    ///
    /// 옛 ProgID 이름(`HWP Document`)은 한컴 같은 다른 제품도 쓸 법한 일반적인 이름이라,
    /// **이름만 보고 지우면 남의 등록을 파괴한다.** `shell\open\command` 가 우리 설치 경로를
    /// 가리킬 때만 우리 것으로 판단한다.
    pub fn migrate_legacy_one(
        classes: &RegKey,
        ext: &str,
        legacy_id: &str,
        legacy_backup: &str,
        prog_id: &str,
        description: &str,
        exe: &Path,
    ) -> LegacyOutcome {
        let Ok(legacy_key) = classes.open_subkey(legacy_id) else {
            return LegacyOutcome::NotFound;
        };

        let command = legacy_key
            .open_subkey("shell\\open\\command")
            .and_then(|k| k.get_value::<String, _>(""))
            .unwrap_or_default()
            .to_lowercase();
        if !command.contains(LEGACY_COMMAND_MARKER) {
            return LegacyOutcome::NotOurs;
        }

        // 옛 키가 들고 있던 "진짜 원본" 값. 이걸 새 백업으로 승계해야, 나중에 사용자가
        // 설정에서 껐을 때 이미 삭제된 옛 ProgID 로 복원되는 사고를 막는다.
        let original = classes
            .open_subkey(format!(".{ext}"))
            .and_then(|k| k.get_value::<String, _>(legacy_backup))
            .unwrap_or_default();

        let owns_default = current_prog_id(classes, ext) == legacy_id;
        let outcome = if owns_default {
            // 사용자는 지금 이 연결을 쓰고 있다. 끊지 말고 새 방식으로 옮긴다.
            if register(classes, ext, prog_id, description, exe).is_err() {
                return LegacyOutcome::NotOurs;
            }
            if let Ok(ext_key) =
                classes.open_subkey_with_flags(format!(".{ext}"), KEY_READ | KEY_WRITE)
            {
                let _ = ext_key.set_value(BACKUP_VALUE, &original);
            }
            LegacyOutcome::Migrated
        } else {
            // 그 사이 다른 프로그램이 기본값을 가져갔다 — 기본값은 그대로 두고 잔재만 지운다.
            LegacyOutcome::RemovedOnly
        };

        if let Ok(ext_key) = classes.open_subkey_with_flags(format!(".{ext}"), KEY_READ | KEY_WRITE)
        {
            let _ = ext_key.delete_value(legacy_backup);
        }
        let _ = classes.delete_subkey_all(legacy_id);

        outcome
    }

    /// 설치 관리자가 남긴 옛 연결을 정리한다. 실패는 무시한다 — 기동을 막지 않는다.
    pub fn migrate_legacy_associations(exe: &Path) -> Vec<LegacyOutcome> {
        let Ok(classes) = classes() else {
            return Vec::new();
        };
        let outcomes: Vec<LegacyOutcome> = LEGACY_PROG_IDS
            .iter()
            .zip(PROG_IDS.iter())
            .map(|((ext, legacy_id, legacy_backup), (_, prog_id, description))| {
                migrate_legacy_one(
                    &classes,
                    ext,
                    legacy_id,
                    legacy_backup,
                    prog_id,
                    description,
                    exe,
                )
            })
            .collect();

        if outcomes.iter().any(|o| *o != LegacyOutcome::NotFound) {
            notify_shell();
        }
        outcomes
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

#[cfg(all(test, windows))]
mod tests {
    use std::path::Path;

    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    use super::windows_impl::{migrate_legacy_one, LegacyOutcome};

    const OUR_EXE: &str = r"C:\Users\tester\AppData\Local\RHWP Studio\app.exe";

    /// 실제 HKCU\Software\Classes 를 건드리지 않도록 테스트 전용 하위 트리를 쓴다.
    struct TestClasses {
        root: String,
        key: RegKey,
    }

    impl TestClasses {
        fn new(name: &str) -> Self {
            let root = format!("Software\\RhwpStudioTest\\{name}");
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let _ = hkcu.delete_subkey_all(&root);
            let (key, _) = hkcu.create_subkey(&root).unwrap();
            let key = hkcu
                .open_subkey_with_flags(&root, KEY_READ | KEY_WRITE)
                .unwrap_or(key);
            Self { root, key }
        }

        /// 옛 설치 관리자가 만들던 모양 그대로 등록한다.
        fn seed_legacy(&self, ext: &str, legacy_id: &str, backup_name: &str, command: &str, original: &str) {
            let (ext_key, _) = self.key.create_subkey(format!(".{ext}")).unwrap();
            ext_key.set_value("", &legacy_id).unwrap();
            ext_key.set_value(backup_name, &original).unwrap();
            let (class_key, _) = self.key.create_subkey(legacy_id).unwrap();
            class_key
                .create_subkey("shell\\open\\command")
                .unwrap()
                .0
                .set_value("", &command)
                .unwrap();
        }

        fn ext_value(&self, ext: &str, name: &str) -> String {
            self.key
                .open_subkey(format!(".{ext}"))
                .and_then(|k| k.get_value::<String, _>(name))
                .unwrap_or_default()
        }

        fn has_key(&self, name: &str) -> bool {
            self.key.open_subkey(name).is_ok()
        }
    }

    impl Drop for TestClasses {
        fn drop(&mut self) {
            let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.root);
        }
    }

    fn migrate(t: &TestClasses) -> LegacyOutcome {
        migrate_legacy_one(
            &t.key,
            "hwp",
            "HWP Document",
            "HWP Document_backup",
            "RhwpStudio.hwp",
            "아래한글 HWP 문서",
            Path::new(OUR_EXE),
        )
    }

    #[test]
    fn 옛_등록이_없으면_아무것도_하지_않는다() {
        let t = TestClasses::new("none");
        assert_eq!(migrate(&t), LegacyOutcome::NotFound);
    }

    #[test]
    fn 우리_등록이면_새_방식으로_옮기고_원본_백업을_승계한다() {
        let t = TestClasses::new("migrate");
        // 설치 전 원래 연결이 한컴이었다고 가정한다.
        t.seed_legacy("hwp", "HWP Document", "HWP Document_backup",
            &format!("\"{OUR_EXE}\" \"%1\""), "Hwp.Document.8");

        assert_eq!(migrate(&t), LegacyOutcome::Migrated);

        assert_eq!(t.ext_value("hwp", ""), "RhwpStudio.hwp");
        // 새 백업은 "HWP Document" 가 아니라 진짜 원본이어야 한다 — 그러지 않으면
        // 나중에 연결을 끌 때 이미 삭제된 옛 ProgID 로 복원된다.
        assert_eq!(t.ext_value("hwp", "RhwpStudio_backup"), "Hwp.Document.8");
        assert_eq!(t.ext_value("hwp", "HWP Document_backup"), "");
        assert!(!t.has_key("HWP Document"), "옛 ProgID 키가 남았다");
        assert!(t.has_key("RhwpStudio.hwp"));
    }

    #[test]
    fn 다른_프로그램의_같은_이름_등록은_건드리지_않는다() {
        let t = TestClasses::new("not-ours");
        t.seed_legacy("hwp", "HWP Document", "HWP Document_backup",
            r#""C:\Program Files\Hnc\Hwp.exe" "%1""#, "");

        assert_eq!(migrate(&t), LegacyOutcome::NotOurs);

        // 기본값도 ProgID 키도 그대로여야 한다.
        assert_eq!(t.ext_value("hwp", ""), "HWP Document");
        assert!(t.has_key("HWP Document"));
        assert!(!t.has_key("RhwpStudio.hwp"));
    }

    #[test]
    fn 기본값을_남이_가져갔으면_잔재만_지운다() {
        let t = TestClasses::new("removed-only");
        t.seed_legacy("hwp", "HWP Document", "HWP Document_backup",
            &format!("\"{OUR_EXE}\" \"%1\""), "");
        // 그 뒤 다른 프로그램이 기본값을 가져간 상황.
        let ext_key = t.key.open_subkey_with_flags(".hwp", KEY_READ | KEY_WRITE).unwrap();
        ext_key.set_value("", &"Hwp.Document.8").unwrap();

        assert_eq!(migrate(&t), LegacyOutcome::RemovedOnly);

        // 남의 기본값을 빼앗지 않는다.
        assert_eq!(t.ext_value("hwp", ""), "Hwp.Document.8");
        assert!(!t.has_key("HWP Document"), "옛 ProgID 키가 남았다");
        assert!(!t.has_key("RhwpStudio.hwp"), "임의로 우리 등록을 만들면 안 된다");
    }
}

/// 설치 관리자(0.8.4 이하)가 등록한 옛 파일 연결을 새 방식으로 옮긴다.
///
/// 기동 시 한 번 호출한다. 이관 후 옛 키가 사라지므로 다음 기동에는 자동으로 무동작이다.
/// 실패해도 조용히 넘어간다 — 파일 연결 정리가 앱 기동을 막아서는 안 된다.
pub fn migrate_legacy_associations_on_startup() {
    #[cfg(windows)]
    {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        for outcome in windows_impl::migrate_legacy_associations(&exe) {
            if outcome != windows_impl::LegacyOutcome::NotFound {
                log::info!("[file-association] 옛 등록 처리: {outcome:?}");
            }
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
