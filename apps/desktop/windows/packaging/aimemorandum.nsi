Unicode true
SetCompressor /SOLID lzma
CRCCheck on

!include "MUI2.nsh"

!ifndef APP_VERSION
  !error "APP_VERSION must be supplied by scripts/build-windows-installer.ps1"
!endif
!ifndef APP_ARCH
  !error "APP_ARCH must be supplied"
!endif
!ifndef FILE_VERSION
  !error "FILE_VERSION must be supplied"
!endif
!ifndef BINARY_PATH
  !error "BINARY_PATH must be supplied"
!endif
!ifndef LICENSE_PATH
  !error "LICENSE_PATH must be supplied"
!endif
!ifndef NOTICES_PATH
  !error "NOTICES_PATH must be supplied"
!endif
!ifndef OUTPUT_PATH
  !error "OUTPUT_PATH must be supplied"
!endif

Name "AIMemorandum ${APP_VERSION}"
OutFile "${OUTPUT_PATH}"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\AIMemorandum"
BrandingText "AIMemorandum ${APP_VERSION} (${APP_ARCH})"
VIProductVersion "${FILE_VERSION}"
VIAddVersionKey "ProductName" "AIMemorandum"
VIAddVersionKey "ProductVersion" "${APP_VERSION}"
VIAddVersionKey "FileDescription" "AIMemorandum directory-only installer"
VIAddVersionKey "FileVersion" "${FILE_VERSION}"
VIAddVersionKey "LegalCopyright" "AIMemorandum contributors; GPLv3"

!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_LICENSE "${LICENSE_PATH}"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Section "AIMemorandum" SEC_MAIN
  SectionIn RO
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  SetOverwrite on
  File /oname=AIMemorandum.exe "${BINARY_PATH}"
  File /oname=LICENSE "${LICENSE_PATH}"
  File /oname=THIRD_PARTY_NOTICES.md "${NOTICES_PATH}"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$INSTDIR\AIMemorandum.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\THIRD_PARTY_NOTICES.md"
  Delete "$INSTDIR\Uninstall.exe"
  SetOutPath "$TEMP"
  RMDir "$INSTDIR"
SectionEnd
