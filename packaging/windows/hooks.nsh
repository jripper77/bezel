; Bezel Evo installer extensions. No application processes are terminated.
!include nsDialogs.nsh
Var EvoStartupCheckbox
Var EvoSensorsCheckbox
Var EvoStartup
Var EvoSensors

Function EvoOptions
  IfSilent done
  ${GetOptions} $CMDLINE "/P" $R0
  ${IfNot} ${Errors}
    Goto done
  ${EndIf}
  !insertmacro MUI_HEADER_TEXT "$(EvoOptionsTitle)" "$(EvoOptionsSubtitle)"
  nsDialogs::Create 1018
  Pop $R0
  ${NSD_CreateLabel} 0 0 100% 36u "$(EvoOptionsText)"
  Pop $R0
  ${NSD_CreateCheckbox} 0 44u 100% 24u "$(EvoStartupText)"
  Pop $EvoStartupCheckbox
  ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "BezelEvoLight"
  ${If} $R0 != ""
    ${NSD_Check} $EvoStartupCheckbox
  ${EndIf}
  ${NSD_CreateCheckbox} 0 76u 100% 30u "$(EvoSensorsText)"
  Pop $EvoSensorsCheckbox
  ${NSD_Check} $EvoSensorsCheckbox
  nsDialogs::Show
  done:
FunctionEnd

Function EvoOptionsLeave
  ${NSD_GetState} $EvoStartupCheckbox $EvoStartup
  ${NSD_GetState} $EvoSensorsCheckbox $EvoSensors
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
  ; In a new install the maintenance script is not yet on disk.
  InitPluginsDir
  File /oname=$PLUGINSDIR\evo-maintenance.ps1 "@EVO_MAINTENANCE@"
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\evo-maintenance.ps1" -Mode Check -AppDirectory "$INSTDIR"'
  Pop $R0
  Pop $R1
  ${If} $R0 != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "$R1" /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode != 1
    ReadRegStr $R0 HKCU "${MANUPRODUCTKEY}" ""
    ${If} $R0 == $INSTDIR
      DeleteRegKey HKCU "${MANUPRODUCTKEY}"
      DeleteRegKey /ifempty HKCU "${MANUKEY}"
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  SetOutPath "$INSTDIR"
  CreateDirectory "$SMPROGRAMS\Bezel Evo"
  CreateShortCut "$SMPROGRAMS\Bezel Evo\Bezel Evo Light.lnk" "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" '-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "$INSTDIR\start-light.ps1"' "$INSTDIR\bezel-studio.exe"
  CreateShortCut "$SMPROGRAMS\Bezel Evo\Setup sensors.lnk" "$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" '-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File "$INSTDIR\installer-maintenance.ps1" -Mode Sensors -AppDirectory "$INSTDIR"' "$INSTDIR\bezel-studio.exe"
  ${IfNot} ${Silent}
  ${AndIf} $PassiveMode != 1
    nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\installer-maintenance.ps1" -Mode Startup -AppDirectory "$INSTDIR" -Startup "$EvoStartup"'
    Pop $R0
    Pop $R1
    ${If} $R0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "$R1" /SD IDOK
    ${EndIf}
    ${If} $EvoSensors == ${BST_CHECKED}
      nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\installer-maintenance.ps1" -Mode Sensors -AppDirectory "$INSTDIR"'
      Pop $R0
      Pop $R1
      ${If} $R0 != 0
        MessageBox MB_OK|MB_ICONEXCLAMATION "$(EvoSensorsFailed)$\r$\n$R1"
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\installer-maintenance.ps1" -Mode Check -AppDirectory "$INSTDIR"'
  Pop $R0
  Pop $R1
  ${If} $R0 != 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "$R1" /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
  ${If} $UpdateMode != 1
    nsExec::ExecToStack '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\installer-maintenance.ps1" -Mode Uninstall -AppDirectory "$INSTDIR"'
    Pop $R0
    Pop $R1
    ${If} $R0 != 0
      MessageBox MB_OK|MB_ICONEXCLAMATION "$R1" /SD IDOK
      SetErrorLevel 1
    Abort
    ${EndIf}
  ${EndIf}
!macroend
