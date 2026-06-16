!macro NSIS_HOOK_POSTINSTALL
  !if "${STARTMENUFOLDER}" != ""
    CreateShortcut "$SMPROGRAMS\$AppStartMenuFolder\Uninstall Chadow Games Launcher.lnk" "$INSTDIR\uninstall.exe"
  !else
    CreateShortcut "$SMPROGRAMS\Uninstall Chadow Games Launcher.lnk" "$INSTDIR\uninstall.exe"
  !endif
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !if "${STARTMENUFOLDER}" != ""
    Delete "$SMPROGRAMS\$AppStartMenuFolder\Uninstall Chadow Games Launcher.lnk"
  !endif
  Delete "$SMPROGRAMS\Uninstall Chadow Games Launcher.lnk"
!macroend
