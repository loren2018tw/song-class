!macro NSIS_HOOK_POSTINSTALL
  SetShellVarContext current

  ; Start menu shortcut with Chinese display name.
  CreateShortCut "$SMPROGRAMS\爽課啦.lnk" "$INSTDIR\song-class.exe"

  ; If the installer created the default English desktop shortcut,
  ; replace it with the Chinese display name.
  IfFileExists "$DESKTOP\song-class.lnk" 0 +3
  Delete "$DESKTOP\song-class.lnk"
  CreateShortCut "$DESKTOP\爽課啦.lnk" "$INSTDIR\song-class.exe"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  SetShellVarContext current

  ; Remove custom Chinese shortcuts.
  Delete "$SMPROGRAMS\爽課啦.lnk"
  Delete "$DESKTOP\爽課啦.lnk"
!macroend
