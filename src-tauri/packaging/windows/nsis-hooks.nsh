!macro NSIS_HOOK_POSTINSTALL
  ; Elevated installs may create shortcuts in all-users scope.
  SetShellVarContext all

  ; Detect existing all-users Start menu shortcut footprint.
  StrCpy $0 "0"
  IfFileExists "$SMPROGRAMS\song-class.lnk" 0 +2
  StrCpy $0 "1"
  IfFileExists "$SMPROGRAMS\song-class\song-class.lnk" 0 +2
  StrCpy $0 "1"
  IfFileExists "$SMPROGRAMS\爽課啦.lnk" 0 +2
  StrCpy $0 "1"
  IfFileExists "$SMPROGRAMS\song-class\爽課啦.lnk" 0 +2
  StrCpy $0 "1"

  ; Detect existing all-users desktop shortcut footprint.
  StrCpy $1 "0"
  IfFileExists "$DESKTOP\song-class.lnk" 0 +2
  StrCpy $1 "1"
  IfFileExists "$DESKTOP\Song Class.lnk" 0 +2
  StrCpy $1 "1"
  IfFileExists "$DESKTOP\爽課啦.lnk" 0 +2
  StrCpy $1 "1"

  ; Proactively remove historical English shortcut paths.
  Delete "$SMPROGRAMS\song-class.lnk"
  Delete "$SMPROGRAMS\song-class\song-class.lnk"
  Delete "$SMPROGRAMS\Song Class.lnk"
  Delete "$SMPROGRAMS\Song Class\Song Class.lnk"
  Delete "$DESKTOP\song-class.lnk"
  Delete "$DESKTOP\Song Class.lnk"
  Delete "$SMPROGRAMS\song-class\爽課啦.lnk"
  Delete "$SMPROGRAMS\Song Class\爽課啦.lnk"

  ; Recreate canonical all-users shortcuts with Chinese display name.
  StrCmp $0 "1" 0 +2
  CreateShortCut "$SMPROGRAMS\爽課啦.lnk" "$INSTDIR\song-class.exe"

  StrCmp $1 "1" 0 +2
  CreateShortCut "$DESKTOP\爽課啦.lnk" "$INSTDIR\song-class.exe"

  ; Per-user installs may create shortcuts in current-user scope.
  SetShellVarContext current

  ; Detect existing current-user Start menu shortcut footprint.
  StrCpy $2 "0"
  IfFileExists "$SMPROGRAMS\song-class.lnk" 0 +2
  StrCpy $2 "1"
  IfFileExists "$SMPROGRAMS\song-class\song-class.lnk" 0 +2
  StrCpy $2 "1"
  IfFileExists "$SMPROGRAMS\爽課啦.lnk" 0 +2
  StrCpy $2 "1"
  IfFileExists "$SMPROGRAMS\song-class\爽課啦.lnk" 0 +2
  StrCpy $2 "1"

  ; Detect existing current-user desktop shortcut footprint.
  StrCpy $3 "0"
  IfFileExists "$DESKTOP\song-class.lnk" 0 +2
  StrCpy $3 "1"
  IfFileExists "$DESKTOP\Song Class.lnk" 0 +2
  StrCpy $3 "1"
  IfFileExists "$DESKTOP\爽課啦.lnk" 0 +2
  StrCpy $3 "1"

  ; Proactively remove historical English shortcut paths.
  Delete "$SMPROGRAMS\song-class.lnk"
  Delete "$SMPROGRAMS\song-class\song-class.lnk"
  Delete "$SMPROGRAMS\Song Class.lnk"
  Delete "$SMPROGRAMS\Song Class\Song Class.lnk"
  Delete "$DESKTOP\song-class.lnk"
  Delete "$DESKTOP\Song Class.lnk"
  Delete "$SMPROGRAMS\song-class\爽課啦.lnk"
  Delete "$SMPROGRAMS\Song Class\爽課啦.lnk"

  ; Recreate canonical current-user shortcuts with Chinese display name.
  StrCmp $2 "1" 0 +2
  CreateShortCut "$SMPROGRAMS\爽課啦.lnk" "$INSTDIR\song-class.exe"

  StrCmp $3 "1" 0 +2
  CreateShortCut "$DESKTOP\爽課啦.lnk" "$INSTDIR\song-class.exe"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  SetShellVarContext all

  ; Remove custom Chinese shortcuts from all-users scope.
  Delete "$SMPROGRAMS\爽課啦.lnk"
  Delete "$SMPROGRAMS\song-class\爽課啦.lnk"
  Delete "$DESKTOP\爽課啦.lnk"

  SetShellVarContext current

  ; Remove custom Chinese shortcuts from current-user scope.
  Delete "$SMPROGRAMS\爽課啦.lnk"
  Delete "$SMPROGRAMS\song-class\爽課啦.lnk"
  Delete "$DESKTOP\爽課啦.lnk"
!macroend
