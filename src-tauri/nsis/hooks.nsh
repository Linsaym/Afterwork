!macro NSIS_HOOK_POSTINSTALL
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
