!ifdef APP_ARM64
!macro customFiles_arm64
  SetOutPath "$INSTDIR"
  File "/oname=${APP_EXECUTABLE_FILENAME}" "$%BP_NSIS_ARM64_UNPACKED_DIR%\${APP_EXECUTABLE_FILENAME}"
  File "$%BP_NSIS_ARM64_UNPACKED_DIR%\*.dll"
!macroend
!endif
