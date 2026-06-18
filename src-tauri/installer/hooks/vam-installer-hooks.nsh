!include nsDialogs.nsh
!include LogicLib.nsh

Var VamStartWithWindowsCheckbox
Var VamAutoStartAudioCheckbox
Var VamPromptAudioSetupCheckbox
Var VamStartWithWindowsState
Var VamAutoStartAudioState
Var VamPromptAudioSetupState
Var VamOptionsTitleText
Var VamOptionsSubtitleText
Var VamStartWithWindowsText
Var VamAutoStartAudioText
Var VamPromptAudioSetupText
Var VamDriverInstallText
Var VamDriverInstallDoneText
Var VamDriverInstallMissingText

Page custom VamOptionsPage VamOptionsPageLeave

Function VamSetOptionTexts
  ${If} $LANGUAGE == 1036
    StrCpy $VamOptionsTitleText "Options de démarrage VirtualAudioMix"
    StrCpy $VamOptionsSubtitleText "Choisissez les valeurs par défaut qui seront appliquées au premier lancement. Vous pourrez tout modifier ensuite dans les paramètres de l'application."
    StrCpy $VamStartWithWindowsText "Démarrer VirtualAudioMix avec Windows"
    StrCpy $VamAutoStartAudioText "Démarrer automatiquement le moteur audio après chargement du driver BAD"
    StrCpy $VamPromptAudioSetupText "Ouvrir l'assistant pour choisir le micro et les enceintes/casque par défaut au premier lancement"
    StrCpy $VamDriverInstallText "Installation du Bubux Audio Driver..."
    StrCpy $VamDriverInstallDoneText "Installation du driver BAD terminée ou déjà présente."
    StrCpy $VamDriverInstallMissingText "Package driver BAD introuvable dans l'installation."
  ${Else}
    StrCpy $VamOptionsTitleText "VirtualAudioMix startup options"
    StrCpy $VamOptionsSubtitleText "Choose first-run defaults. You can change everything later in the application settings."
    StrCpy $VamStartWithWindowsText "Start VirtualAudioMix with Windows"
    StrCpy $VamAutoStartAudioText "Automatically start the audio engine after the BAD driver is ready"
    StrCpy $VamPromptAudioSetupText "Open the default microphone and speaker/headphones setup on first launch"
    StrCpy $VamDriverInstallText "Installing Bubux Audio Driver..."
    StrCpy $VamDriverInstallDoneText "BAD driver installation completed or already present."
    StrCpy $VamDriverInstallMissingText "BAD driver package not found in installation directory."
  ${EndIf}
FunctionEnd

Function VamOptionsPage
  IfSilent 0 +2
    Abort

  Call VamSetOptionTexts

  nsDialogs::Create 1018
  Pop $0
  ${If} $0 == error
    Abort
  ${EndIf}

  !insertmacro MUI_HEADER_TEXT "$VamOptionsTitleText" "$VamOptionsSubtitleText"

  ${NSD_CreateLabel} 0 0 100% 28u "$VamOptionsSubtitleText"
  Pop $1

  ${NSD_CreateCheckbox} 12u 45u 92% 12u "$VamStartWithWindowsText"
  Pop $VamStartWithWindowsCheckbox
  SendMessage $VamStartWithWindowsCheckbox ${BM_SETCHECK} ${BST_CHECKED} 0

  ${NSD_CreateCheckbox} 12u 68u 92% 12u "$VamAutoStartAudioText"
  Pop $VamAutoStartAudioCheckbox
  SendMessage $VamAutoStartAudioCheckbox ${BM_SETCHECK} ${BST_CHECKED} 0

  ${NSD_CreateCheckbox} 12u 91u 92% 22u "$VamPromptAudioSetupText"
  Pop $VamPromptAudioSetupCheckbox
  SendMessage $VamPromptAudioSetupCheckbox ${BM_SETCHECK} ${BST_CHECKED} 0

  nsDialogs::Show
FunctionEnd

Function VamOptionsPageLeave
  ${NSD_GetState} $VamStartWithWindowsCheckbox $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $VamStartWithWindowsState 1
  ${Else}
    StrCpy $VamStartWithWindowsState 0
  ${EndIf}

  ${NSD_GetState} $VamAutoStartAudioCheckbox $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $VamAutoStartAudioState 1
  ${Else}
    StrCpy $VamAutoStartAudioState 0
  ${EndIf}

  ${NSD_GetState} $VamPromptAudioSetupCheckbox $0
  ${If} $0 == ${BST_CHECKED}
    StrCpy $VamPromptAudioSetupState 1
  ${Else}
    StrCpy $VamPromptAudioSetupState 0
  ${EndIf}
FunctionEnd

!macro NSIS_HOOK_POSTINSTALL
  Call VamSetOptionTexts

  ${If} $VamStartWithWindowsState == ""
    StrCpy $VamStartWithWindowsState 1
  ${EndIf}
  ${If} $VamAutoStartAudioState == ""
    StrCpy $VamAutoStartAudioState 1
  ${EndIf}
  ${If} $VamPromptAudioSetupState == ""
    StrCpy $VamPromptAudioSetupState 1
  ${EndIf}

  WriteRegDWORD HKCU "Software\Bruno Del piero\VirtualAudioMix" "InstallerStartWithWindows" $VamStartWithWindowsState
  WriteRegDWORD HKCU "Software\Bruno Del piero\VirtualAudioMix" "InstallerAutoStartAudio" $VamAutoStartAudioState
  WriteRegDWORD HKCU "Software\Bruno Del piero\VirtualAudioMix" "InstallerPromptAudioSetup" $VamPromptAudioSetupState

  ${If} $VamStartWithWindowsState = 1
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}" '"$INSTDIR\${MAINBINARYNAME}.exe"'
  ${Else}
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCTNAME}"
  ${EndIf}

  DetailPrint "$VamDriverInstallText"
  ${If} ${FileExists} "$INSTDIR\resources\installer\install-bad-driver.ps1"
    nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\resources\installer\install-bad-driver.ps1"'
    Pop $0
    DetailPrint "$VamDriverInstallDoneText"
  ${Else}
    DetailPrint "$VamDriverInstallMissingText"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} ${FileExists} "$INSTDIR\resources\installer\uninstall-bad-driver.ps1"
    nsExec::ExecToLog 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\resources\installer\uninstall-bad-driver.ps1"'
    Pop $0
  ${EndIf}
!macroend
