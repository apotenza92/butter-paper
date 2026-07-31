!define BP_PDF_PROG_ID "${APP_ID}.pdf"
!define BP_CAPABILITIES_KEY "Software\${APP_ID}\Capabilities"

!macro customInstall
  WriteRegNone SHELL_CONTEXT "Software\Classes\.pdf\OpenWithProgids" "${BP_PDF_PROG_ID}"
  WriteRegStr SHELL_CONTEXT "Software\Classes\${BP_PDF_PROG_ID}" "" "PDF document"
  WriteRegStr SHELL_CONTEXT "Software\Classes\${BP_PDF_PROG_ID}\DefaultIcon" "" "$\"$INSTDIR\${APP_EXECUTABLE_FILENAME}$\",0"
  WriteRegStr SHELL_CONTEXT "Software\Classes\${BP_PDF_PROG_ID}\shell\open\command" "" "$\"$INSTDIR\${APP_EXECUTABLE_FILENAME}$\" $\"%1$\""
  WriteRegStr SHELL_CONTEXT "Software\Classes\Applications\${APP_EXECUTABLE_FILENAME}" "FriendlyAppName" "${PRODUCT_NAME}"
  WriteRegStr SHELL_CONTEXT "Software\Classes\Applications\${APP_EXECUTABLE_FILENAME}\SupportedTypes" ".pdf" ""
  WriteRegStr SHELL_CONTEXT "${BP_CAPABILITIES_KEY}" "ApplicationName" "${PRODUCT_NAME}"
  WriteRegStr SHELL_CONTEXT "${BP_CAPABILITIES_KEY}" "ApplicationDescription" "Cross-platform PDF review and markup"
  WriteRegStr SHELL_CONTEXT "${BP_CAPABILITIES_KEY}\FileAssociations" ".pdf" "${BP_PDF_PROG_ID}"
  WriteRegStr SHELL_CONTEXT "Software\RegisteredApplications" "${PRODUCT_NAME}" "${BP_CAPABILITIES_KEY}"
  System::Call "shell32::SHChangeNotify(i, i, i, i) v (0x08000000, 0, 0, 0)"
!macroend

!macro customUnInstall
  DeleteRegValue SHELL_CONTEXT "Software\Classes\.pdf\OpenWithProgids" "${BP_PDF_PROG_ID}"
  DeleteRegKey SHELL_CONTEXT "Software\Classes\${BP_PDF_PROG_ID}"
  DeleteRegKey SHELL_CONTEXT "Software\Classes\Applications\${APP_EXECUTABLE_FILENAME}"
  DeleteRegKey SHELL_CONTEXT "${BP_CAPABILITIES_KEY}"
  DeleteRegValue SHELL_CONTEXT "Software\RegisteredApplications" "${PRODUCT_NAME}"
  ${ifNot} ${isUpdated}
    System::Call "shell32::SHChangeNotify(i, i, i, i) v (0x08000000, 0, 0, 0)"
  ${endIf}
!macroend
