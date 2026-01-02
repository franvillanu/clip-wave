; Custom NSIS hooks for Clip Wave installer
; This file adds custom uninstall logic to properly clean up app data
; Ensure the finish-page desktop shortcut checkbox is unchecked by default.
!define MUI_FINISHPAGE_SHOWREADME_NOTCHECKED

!macro NSIS_HOOK_PREUNINSTALL
  ; Always delete app data on uninstall (checkbox is hidden below).
  StrCpy $DeleteAppDataCheckboxState 1
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Close the installer after files are installed (skip the "Next" step).
  SetAutoClose true
!macroend

!ifdef __NSD_CheckBox_STYLE
  !undef __NSD_CheckBox_STYLE
!endif
!define __NSD_CheckBox_STYLE 0x40000000

!macro NSIS_HOOK_POSTUNINSTALL
  ; Delete the actual app data directory which uses the product name "Clip Wave"
  ; instead of the bundle ID "com.clipwave.app"
  ${If} $UpdateMode <> 1
    ; The app stores its data in "$LOCALAPPDATA\Clip Wave", not "$LOCALAPPDATA\${BUNDLEID}"
    SetShellVarContext current

    ; Try to delete the "Clip Wave" directory in LocalAppData
    DetailPrint "Removing app data from $LOCALAPPDATA\Clip Wave..."
    ${If} ${FileExists} "$LOCALAPPDATA\Clip Wave\*.*"
      ; Delete files one by one to avoid issues with locked files
      ClearErrors
      RmDir /r "$LOCALAPPDATA\Clip Wave"
      ${If} ${Errors}
        DetailPrint "Warning: Some files in $LOCALAPPDATA\Clip Wave could not be removed (may be in use)"
        DetailPrint "You can manually delete this folder after rebooting"
      ${Else}
        DetailPrint "Successfully removed $LOCALAPPDATA\Clip Wave"
      ${EndIf}
    ${Else}
      DetailPrint "App data folder $LOCALAPPDATA\Clip Wave does not exist, skipping"
    ${EndIf}

    ; Also try to delete from APPDATA if it exists (for bundle ID directory)
    ${If} ${FileExists} "$APPDATA\com.clipwave.app\*.*"
      DetailPrint "Removing app data from $APPDATA\com.clipwave.app..."
      ClearErrors
      RmDir /r "$APPDATA\com.clipwave.app"
      ${If} ${Errors}
        DetailPrint "Warning: Some files in $APPDATA\com.clipwave.app could not be removed"
      ${Else}
        DetailPrint "Successfully removed $APPDATA\com.clipwave.app"
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend
