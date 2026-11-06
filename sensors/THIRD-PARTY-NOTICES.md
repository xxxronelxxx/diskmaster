# Third-party notices — sensor module

The temperature/fan module (`DiskMasterSensors.exe` and the DLLs next to it)
uses the following components. The main application ships no PowerShell and no
GPL code of its own.

## LibreHardwareMonitorLib 0.9.6

- Project: https://github.com/LibreHardwareMonitor/LibreHardwareMonitor
- License: Mozilla Public License 2.0 (`licenses/MPL-2.0.txt`)
- Usage: unmodified NuGet package. Source code is available at the project link.

## PawnIO 2.2.0 (optional component, installed only on request)

- Project: https://github.com/namazso/PawnIO
- Installer: https://github.com/namazso/PawnIO.Setup/releases/tag/2.2.0
- License: GNU General Public License v2.0 with the PawnIO linking exception
  (`licenses/GPL-2.0.txt`)
- Usage: the official signed installer is redistributed unmodified. DiskMaster
  talks to the driver only through its device IOCTL interface, which the
  exception explicitly allows for independent modules. The source of the driver
  is available at the project link.

## Other managed dependencies

`HidSharp`, `DiskInfoToolkit`, `RAMSPDToolkit-NDD`, `BlackSharp.Core`,
`System.Management`, `System.Memory`, `System.Threading.AccessControl` and their
transitive dependencies are distributed by their respective authors under their
own licenses; the package licenses are available in the NuGet package cache and
at the projects' repositories.

The sensor module is not loaded automatically. It runs only after the user
presses the corresponding button, and PawnIO is installed only after an explicit
confirmation.
