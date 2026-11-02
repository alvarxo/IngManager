; IngManager NSIS hooks
;
; Uninstall MUST NOT delete:
; - User project roots (engineering files outside the install dir)
; - App local data (%LOCALAPPDATA%\IngManager) — DB, settings, FTS index
;
; Tauri's default NSIS template only removes $INSTDIR and shortcuts/registry.
; These hooks deliberately do nothing destructive so that reinstall/update
; preserves root_path and SQLite state.

!macro NSIS_HOOK_PREINSTALL
!macroend

!macro NSIS_HOOK_POSTINSTALL
!macroend

!macro NSIS_HOOK_PREUNINSTALL
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
!macroend
