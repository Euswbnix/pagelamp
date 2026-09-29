; PageLamp NSIS installer hooks (tauri.conf.json → bundle.windows.nsis.installerHooks).
;
; An AI app (Claude Desktop, Codex…) keeps `pagelamp.exe mcp` running, and Windows won't let the
; installer overwrite a running .exe, but it can be renamed. Tauri's installer only checks
; whether the main exe runs, so before files are copied (and before uninstalling), a running
; sidecar is moved aside. PageLamp deletes the leftover at its next start
; (src/updates.rs, remove_old_sidecar); the AI app keeps using its old process until the
; student reopens it, which the app asks them to do.
;
; UNTESTED on Windows so far: the owner tests it on a Windows 11 PC with Claude Desktop running
; before it ships to students (plan M0.4).

!macro PAGELAMP_MOVE_SIDECAR_ASIDE
  ${If} ${FileExists} "$INSTDIR\pagelamp.exe"
    ; A leftover from an earlier update may still be running as well: then use a second name.
    Delete "$INSTDIR\pagelamp.exe.old"
    ${If} ${FileExists} "$INSTDIR\pagelamp.exe.old"
      Delete "$INSTDIR\pagelamp.exe.old2"
      Rename "$INSTDIR\pagelamp.exe" "$INSTDIR\pagelamp.exe.old2"
    ${Else}
      Rename "$INSTDIR\pagelamp.exe" "$INSTDIR\pagelamp.exe.old"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro PAGELAMP_MOVE_SIDECAR_ASIDE
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro PAGELAMP_MOVE_SIDECAR_ASIDE
  ; Nothing will start PageLamp again to clean up: remove the leftovers now or at reboot.
  Delete /REBOOTOK "$INSTDIR\pagelamp.exe.old"
  Delete /REBOOTOK "$INSTDIR\pagelamp.exe.old2"
!macroend
