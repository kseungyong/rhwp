; RHWP Studio NSIS 설치 훅
;
; 자체 서명 인증서를 "신뢰할 수 있는 게시자"와 "신뢰할 수 있는 루트 인증 기관" 저장소에 등록해,
; 설치 이후 실행 시 SmartScreen이 게시자 미확인으로 표시하지 않도록 한다.
; 사내에 AD/GPO 일괄 배포 체계가 없어(설계 스펙 참고) 이 등록을 설치 과정 안에 포함시켰다.
;
; -user 를 반드시 붙인다. 설치는 currentUser 모드(관리자 권한 없음)인데 -user 없이 호출하면
; certutil 이 machine 저장소에 쓰려 하고, 권한이 없어 조용히 실패한다. 그러면 설치는 성공한
; 것처럼 끝나지만 인증서가 등록되지 않아 실행 시 SmartScreen 경고가 그대로 뜬다.
;
; 인증서 등록이 실패해도 설치 자체는 중단하지 않는다 — 앱은 정상 동작하고 경고만 남는다.
; 다만 반환 코드를 확인해 실패 사실을 설치 로그에 남긴다(무음 실패 방지).

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "RHWP Studio 인증서를 현재 사용자 저장소에 등록합니다..."

  ExecWait '"$SYSDIR\certutil.exe" -user -addstore "TrustedPublisher" "$INSTDIR\rhwp-studio-cert.cer"' $0
  ${If} $0 != 0
    DetailPrint "경고: TrustedPublisher 등록 실패 (certutil 코드 $0). 최초 실행 시 SmartScreen 경고가 표시될 수 있습니다."
  ${EndIf}

  ExecWait '"$SYSDIR\certutil.exe" -user -addstore "Root" "$INSTDIR\rhwp-studio-cert.cer"' $1
  ${If} $1 != 0
    DetailPrint "경고: Root 등록 실패 (certutil 코드 $1). 최초 실행 시 SmartScreen 경고가 표시될 수 있습니다."
  ${EndIf}
!macroend
