; Tauri NSIS hooks for Диск-Мастер.
; Keep the machine clean when the app is upgraded or removed: stop the app and
; the elevated sensor service, remove the scheduled tasks created by the app and
; the per-user autostart entries.

!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /F /IM DiskMasterSensors.exe'
  nsExec::Exec 'taskkill /F /IM DiskMaster.exe'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /F /IM DiskMasterSensors.exe'
  nsExec::Exec 'taskkill /F /IM DiskMaster.exe'
  nsExec::Exec 'schtasks /Delete /TN "DiskMaster\DiskMasterAutoClean" /F'
  nsExec::Exec 'schtasks /Delete /TN "DiskMasterAutostart" /F'
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "DiskMaster"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "DiskMasterWidget"
!macroend
