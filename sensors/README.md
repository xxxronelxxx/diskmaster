# DiskMasterSensors

Managed host for the temperature/fan module. It is a separate process: the main
Rust application never loads .NET assemblies and never runs elevated for sensor
reads.

- **Runtime**: .NET Framework 4.8 (present in Windows 11), x64.
- **Library**: LibreHardwareMonitorLib 0.9.6, unmodified NuGet package.
- **Optional driver**: PawnIO 2.2.0 (signed). It is installed only after the
  user presses the button in the UI and confirms the UAC prompt. The app never
  installs it silently on startup.
- **AMD Ryzen Master**: if `AMDRyzenMasterMonitoringDLL.dll` is already present
  (installed by AMD), it is used as an additional CPU temperature source. It is
  never redistributed.

## Modes

```text
DiskMasterSensors.exe --status --out status.json
DiskMasterSensors.exe --once --out once.json [--parent <pid>]
DiskMasterSensors.exe --serve --out sensors.json --stop sensors.stop --parent <pid>
DiskMasterSensors.exe --install-pawnio --serve --setup PawnIO_setup.exe --install-out install.json --out sensors.json --stop sensors.stop --parent <pid>
```

`--serve` writes an atomic snapshot every 3 seconds and exits when the parent
process disappears or the stop file appears.

## Build

```bash
pnpm exec tsx scripts/build-sensors.ts
```

Requires the .NET SDK (8 or newer). The script publishes the project, copies the
licenses and downloads the official signed PawnIO installer with a pinned
SHA-256 into `src-tauri/resources/sensors/` (gitignored).

## Licensing

See `THIRD-PARTY-NOTICES.md` and `licenses/`. LibreHardwareMonitor is MPL-2.0.
PawnIO is GPL-2.0 with the linking exception for independent modules that talk
to the driver over its IOCTL interface. The PawnIO installer is redistributed
unmodified; its source is linked in the notices.
