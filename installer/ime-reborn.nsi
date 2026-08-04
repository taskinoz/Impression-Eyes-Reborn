Unicode True

!include "MUI2.nsh"
!include "Sections.nsh"
!include "LogicLib.nsh"

!ifndef VERSION
  !define VERSION "0.1.0"
!endif
!ifndef VIEWER_EXE
  !define VIEWER_EXE "..\target\release\ime-reborn.exe"
!endif

!define PRODUCT_NAME "ime-reborn"
!define PRODUCT_PUBLISHER "ime-reborn contributors"
!define PRODUCT_WEB "https://github.com/taskinoz/Impression-Eyes-Reborn"
!define PRODUCT_KEY "Software\ime-reborn"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\ime-reborn"

Name "${PRODUCT_NAME} ${VERSION}"
OutFile "..\dist\ime-reborn-v${VERSION}-windows-x86_64-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\ime-reborn"
InstallDirRegKey HKCU "${PRODUCT_KEY}" "InstallLocation"
RequestExecutionLevel user
SetCompressor /SOLID lzma
SetCompressorDictSize 32
BrandingText "ime-reborn"
Icon "..\assets\ime-reborn.ico"
UninstallIcon "..\assets\ime-reborn.ico"
ShowInstDetails show
ShowUninstDetails show

!define MUI_ABORTWARNING
!define MUI_ICON "..\assets\ime-reborn.ico"
!define MUI_UNICON "..\assets\ime-reborn.ico"
!define MUI_COMPONENTSPAGE_SMALLDESC
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\ime-reborn.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Launch ime-reborn"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=${LANG_ENGLISH} "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "FileVersion" "${VERSION}.0"
VIAddVersionKey /LANG=${LANG_ENGLISH} "CompanyName" "${PRODUCT_PUBLISHER}"
VIAddVersionKey /LANG=${LANG_ENGLISH} "FileDescription" "${PRODUCT_NAME} installer"
VIAddVersionKey /LANG=${LANG_ENGLISH} "LegalCopyright" "ime-reborn contributors"

Function EnsureViewerClosed
  check_again:
    FindWindow $0 "ImpressionEyesRebornWindow"
    ${If} $0 == 0
      Return
    ${EndIf}
    IfSilent silent_failure
    MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION \
      "ime-reborn is currently open. Close every viewer window, then choose Retry." \
      IDRETRY check_again
    Abort
  silent_failure:
    SetErrorLevel 2
    Abort "ime-reborn is running"
FunctionEnd

Section "ime-reborn viewer (required)" ViewerSection
  SectionIn RO
  Call EnsureViewerClosed
  SetOutPath "$INSTDIR"
  File /oname=ime-reborn.exe "${VIEWER_EXE}"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\ime-reborn"
  CreateShortcut "$SMPROGRAMS\ime-reborn\ime-reborn.lnk" "$INSTDIR\ime-reborn.exe" "" "$INSTDIR\ime-reborn.exe" 0
  CreateShortcut "$SMPROGRAMS\ime-reborn\Uninstall ime-reborn.lnk" "$INSTDIR\uninstall.exe"

  WriteRegStr HKCU "${PRODUCT_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Classes\Applications\ime-reborn.exe\shell\open\command" "" '$\"$INSTDIR\ime-reborn.exe$\" $\"%1$\"'
  WriteRegStr HKCU "Software\RegisteredApplications" "ime-reborn" "${PRODUCT_KEY}\Capabilities"
  WriteRegStr HKCU "${PRODUCT_KEY}\Capabilities" "ApplicationName" "ime-reborn"
  WriteRegStr HKCU "${PRODUCT_KEY}\Capabilities" "ApplicationDescription" "A tiny, fast image viewer for Windows"

  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "ime-reborn"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "${PRODUCT_WEB}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\ime-reborn.exe,0"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '$\"$INSTDIR\uninstall.exe$\"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '$\"$INSTDIR\uninstall.exe$\" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

!ifdef UPDATER_EXE
Section /o "Automatic update checks" UpdaterSection
  SetOutPath "$INSTDIR"
  File /oname=ime-reborn-updater.exe "${UPDATER_EXE}"
  ExecWait '$\"$INSTDIR\ime-reborn-updater.exe$\" --install-task' $0
  ${If} $0 != 0
    DetailPrint "The updater was installed, but scheduled-task registration returned $0."
  ${EndIf}
SectionEnd
!endif

SectionGroup /e "File associations (optional)" AssociationsGroup
  !macro AssociationSection SECTION_ID TITLE PROG_ID DESCRIPTION CONTENT_TYPE
    Section /o "${TITLE}" ${SECTION_ID}
      WriteRegStr HKCU "Software\Classes\${PROG_ID}" "" "${DESCRIPTION}"
      WriteRegStr HKCU "Software\Classes\${PROG_ID}\DefaultIcon" "" "$INSTDIR\ime-reborn.exe,0"
      WriteRegStr HKCU "Software\Classes\${PROG_ID}\shell\open\command" "" '$\"$INSTDIR\ime-reborn.exe$\" $\"%1$\"'
    SectionEnd
  !macroend

  !insertmacro AssociationSection AssocJpeg "JPEG images (.jpg, .jpeg)" "ime-reborn.jpeg" "JPEG image" "image/jpeg"
  !insertmacro AssociationSection AssocPng "PNG images (.png)" "ime-reborn.png" "PNG image" "image/png"
  !insertmacro AssociationSection AssocGif "GIF images (.gif)" "ime-reborn.gif" "GIF image" "image/gif"
  !insertmacro AssociationSection AssocWebp "WebP images (.webp)" "ime-reborn.webp" "WebP image" "image/webp"
  !insertmacro AssociationSection AssocBmp "Bitmap images (.bmp)" "ime-reborn.bmp" "Bitmap image" "image/bmp"
  !insertmacro AssociationSection AssocTiff "TIFF images (.tif, .tiff)" "ime-reborn.tiff" "TIFF image" "image/tiff"
  !insertmacro AssociationSection AssocPnm "Portable anymap (.pnm, .pbm, .pgm, .ppm)" "ime-reborn.pnm" "Portable anymap image" "image/x-portable-anymap"
  !insertmacro AssociationSection AssocDds "DirectDraw surfaces (.dds)" "ime-reborn.dds" "DirectDraw surface" "image/vnd-ms.dds"
  !insertmacro AssociationSection AssocFarbfeld "Farbfeld images (.ff)" "ime-reborn.ff" "Farbfeld image" "application/octet-stream"
  !insertmacro AssociationSection AssocIco "Windows icons (.ico)" "ime-reborn.ico" "Windows icon" "image/x-icon"
  !insertmacro AssociationSection AssocQoi "Quite OK Image (.qoi)" "ime-reborn.qoi" "Quite OK Image" "image/qoi"
  !insertmacro AssociationSection AssocTga "Targa images (.tga)" "ime-reborn.tga" "Targa image" "image/x-tga"
SectionGroupEnd

!macro RegisterExtension SECTION_ID EXT PROG_ID CONTENT_TYPE
  SectionGetFlags ${SECTION_ID} $0
  IntOp $0 $0 & ${SF_SELECTED}
  ${If} $0 != 0
    WriteRegStr HKCU "Software\Classes\.${EXT}\OpenWithProgids" "${PROG_ID}" ""
    WriteRegStr HKCU "Software\Classes\.${EXT}" "Content Type" "${CONTENT_TYPE}"
    WriteRegStr HKCU "${PRODUCT_KEY}\Capabilities\FileAssociations" ".${EXT}" "${PROG_ID}"
  ${EndIf}
!macroend

Section -FinalizeAssociations
  !insertmacro RegisterExtension ${AssocJpeg} "jpg" "ime-reborn.jpeg" "image/jpeg"
  !insertmacro RegisterExtension ${AssocJpeg} "jpeg" "ime-reborn.jpeg" "image/jpeg"
  !insertmacro RegisterExtension ${AssocPng} "png" "ime-reborn.png" "image/png"
  !insertmacro RegisterExtension ${AssocGif} "gif" "ime-reborn.gif" "image/gif"
  !insertmacro RegisterExtension ${AssocWebp} "webp" "ime-reborn.webp" "image/webp"
  !insertmacro RegisterExtension ${AssocBmp} "bmp" "ime-reborn.bmp" "image/bmp"
  !insertmacro RegisterExtension ${AssocTiff} "tif" "ime-reborn.tiff" "image/tiff"
  !insertmacro RegisterExtension ${AssocTiff} "tiff" "ime-reborn.tiff" "image/tiff"
  !insertmacro RegisterExtension ${AssocPnm} "pnm" "ime-reborn.pnm" "image/x-portable-anymap"
  !insertmacro RegisterExtension ${AssocPnm} "pbm" "ime-reborn.pnm" "image/x-portable-bitmap"
  !insertmacro RegisterExtension ${AssocPnm} "pgm" "ime-reborn.pnm" "image/x-portable-graymap"
  !insertmacro RegisterExtension ${AssocPnm} "ppm" "ime-reborn.pnm" "image/x-portable-pixmap"
  !insertmacro RegisterExtension ${AssocDds} "dds" "ime-reborn.dds" "image/vnd-ms.dds"
  !insertmacro RegisterExtension ${AssocFarbfeld} "ff" "ime-reborn.ff" "application/octet-stream"
  !insertmacro RegisterExtension ${AssocIco} "ico" "ime-reborn.ico" "image/x-icon"
  !insertmacro RegisterExtension ${AssocQoi} "qoi" "ime-reborn.qoi" "image/qoi"
  !insertmacro RegisterExtension ${AssocTga} "tga" "ime-reborn.tga" "image/x-tga"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd

Section "Uninstall"
  FindWindow $0 "ImpressionEyesRebornWindow"
  ${If} $0 != 0
    IfSilent silent_uninstall_failure
    MessageBox MB_OK|MB_ICONEXCLAMATION "Close every ime-reborn viewer window before uninstalling."
    Abort
    silent_uninstall_failure:
      SetErrorLevel 2
      Abort "ime-reborn is running"
  ${EndIf}

  IfFileExists "$INSTDIR\ime-reborn-updater.exe" 0 updater_removed
    ExecWait '$\"$INSTDIR\ime-reborn-updater.exe$\" --remove-task' $0
    Delete "$INSTDIR\ime-reborn-updater.exe"
  updater_removed:

  !macro UnregisterExtension EXT PROG_ID
    DeleteRegValue HKCU "Software\Classes\.${EXT}\OpenWithProgids" "${PROG_ID}"
  !macroend
  !insertmacro UnregisterExtension "jpg" "ime-reborn.jpeg"
  !insertmacro UnregisterExtension "jpeg" "ime-reborn.jpeg"
  !insertmacro UnregisterExtension "png" "ime-reborn.png"
  !insertmacro UnregisterExtension "gif" "ime-reborn.gif"
  !insertmacro UnregisterExtension "webp" "ime-reborn.webp"
  !insertmacro UnregisterExtension "bmp" "ime-reborn.bmp"
  !insertmacro UnregisterExtension "tif" "ime-reborn.tiff"
  !insertmacro UnregisterExtension "tiff" "ime-reborn.tiff"
  !insertmacro UnregisterExtension "pnm" "ime-reborn.pnm"
  !insertmacro UnregisterExtension "pbm" "ime-reborn.pnm"
  !insertmacro UnregisterExtension "pgm" "ime-reborn.pnm"
  !insertmacro UnregisterExtension "ppm" "ime-reborn.pnm"
  !insertmacro UnregisterExtension "dds" "ime-reborn.dds"
  !insertmacro UnregisterExtension "ff" "ime-reborn.ff"
  !insertmacro UnregisterExtension "ico" "ime-reborn.ico"
  !insertmacro UnregisterExtension "qoi" "ime-reborn.qoi"
  !insertmacro UnregisterExtension "tga" "ime-reborn.tga"

  DeleteRegKey HKCU "Software\Classes\ime-reborn.jpeg"
  DeleteRegKey HKCU "Software\Classes\ime-reborn.png"
  DeleteRegKey HKCU "Software\Classes\ime-reborn.gif"
  DeleteRegKey HKCU "Software\Classes\ime-reborn.webp"
  DeleteRegKey HKCU "Software\Classes\ime-reborn.bmp"
  DeleteRegKey HKCU "Software\Classes\ime-reborn.tiff"
  DeleteRegKey HKCU "Software\Classes\ime-reborn.pnm"
  DeleteRegKey HKCU "Software\Classes\ime-reborn.dds"
  DeleteRegKey HKCU "Software\Classes\ime-reborn.ff"
  DeleteRegKey HKCU "Software\Classes\ime-reborn.ico"
  DeleteRegKey HKCU "Software\Classes\ime-reborn.qoi"
  DeleteRegKey HKCU "Software\Classes\ime-reborn.tga"
  DeleteRegKey HKCU "Software\Classes\Applications\ime-reborn.exe"
  DeleteRegValue HKCU "Software\RegisteredApplications" "ime-reborn"
  DeleteRegKey HKCU "${PRODUCT_KEY}"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"

  Delete "$INSTDIR\ime-reborn.exe"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\ime-reborn\ime-reborn.lnk"
  Delete "$SMPROGRAMS\ime-reborn\Uninstall ime-reborn.lnk"
  RMDir "$SMPROGRAMS\ime-reborn"
  RMDir "$INSTDIR"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
SectionEnd

LangString DESC_Viewer ${LANG_ENGLISH} "Installs the lightweight image viewer."
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${ViewerSection} $(DESC_Viewer)
!ifdef UPDATER_EXE
  !insertmacro MUI_DESCRIPTION_TEXT ${UpdaterSection} "Installs a separate short-lived updater and opts in to scheduled GitHub release checks."
!endif
  !insertmacro MUI_DESCRIPTION_TEXT ${AssociationsGroup} "Select the image families that should offer ime-reborn in Windows Open with choices."
!insertmacro MUI_FUNCTION_DESCRIPTION_END
