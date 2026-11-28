!define AFTERWORK_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define MUI_CUSTOMFUNCTION_GUIINIT RememberAutostart
Var AutostartChoice

Function RememberAutostart
  ; The maintenance page can uninstall the previous version and remove its Run entry.
  ReadRegStr $AutostartChoice HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Afterwork" "UninstallString"
  ${If} $AutostartChoice == ""
    StrCpy $AutostartChoice "fresh"
  ${Else}
    ReadRegStr $AutostartChoice HKCU "${AFTERWORK_RUN_KEY}" "Afterwork"
    ${If} $AutostartChoice != ""
      StrCpy $AutostartChoice "enabled"
    ${EndIf}
  ${EndIf}
  ClearErrors
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  ${If} ${Silent}
    Call RememberAutostart
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $UpdateMode <> 1
    ${If} $AutostartChoice == "fresh"
      ; Windows can retain this setting after a previous uninstall.
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"
      ClearErrors
    ${EndIf}
    ${If} $AutostartChoice != ""
      WriteRegStr HKCU "${AFTERWORK_RUN_KEY}" "${PRODUCTNAME}" '"$INSTDIR\${MAINBINARYNAME}.exe" --background'
    ${EndIf}
  ${EndIf}

  ${If} $UpdateMode = 1
    ${GetOptions} $CMDLINE "/R" $R0
    ${IfNot} ${Errors}
      ${GetOptions} $CMDLINE "/ARGS" $R0
      ${If} $R0 == "--background"
        ; Tauri reads /ARGS from $CMDLINE again in .onInstSuccess.
        StrCpy $CMDLINE '"$EXEPATH" /UPDATE /R /ARGS'
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend
