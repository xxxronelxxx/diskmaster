using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
using System.Threading;
using LibreHardwareMonitor.Hardware;
using LibreHardwareMonitor.PawnIo;

namespace DiskMasterSensors
{
    internal static class Program
    {
        private static int Main(string[] args)
        {
            var mode = args.Length > 0 ? args[0] : string.Empty;
            var outPath = Arg(args, "--out");

            try
            {
                switch (mode)
                {
                    case "--status":
                        return Write(outPath, StatusJson());
                    case "--once":
                        return Write(outPath, ReadOnceJson(ArgInt(args, "--parent")));
                    case "--serve":
                        return Serve(args, outPath);
                    case "--install-pawnio":
                    {
                        var installCode = InstallPawnIo(args, outPath);
                        // One UAC prompt: install and keep serving in the same
                        // elevated process.
                        if (Arg(args, "--serve") != null && !string.IsNullOrEmpty(outPath))
                            return Serve(args, outPath);
                        return installCode;
                    }
                    default:
                        return Write(outPath, ErrorJson("Неизвестный режим: " + mode));
                }
            }
            catch (Exception error)
            {
                return Write(outPath, ErrorJson(error.ToString()));
            }
        }

        /* ------------------------------------------------------------------ */
        /*                                 modes                              */
        /* ------------------------------------------------------------------ */

        private static string StatusJson()
        {
            var builder = new StringBuilder();
            builder.Append('{');
            builder.Append("\"ok\":true,");
            builder.Append("\"admin\":").Append(IsAdmin() ? "true" : "false").Append(',');
            builder.Append("\"pawnioInstalled\":").Append(PawnIo.IsInstalled ? "true" : "false").Append(',');
            builder.Append("\"pawnioVersion\":").Append(PawnIo.Version == null ? "null" : JsonString(PawnIo.Version.ToString())).Append(',');
            builder.Append("\"library\":").Append(JsonString(typeof(Computer).Assembly.GetName().Version?.ToString() ?? "0.9.6")).Append(',');
            builder.Append("\"amdSdk\":").Append(FindAmdSdk() == null ? "null" : JsonString(FindAmdSdk())).Append(',');
            builder.Append("\"exe\":").Append(JsonString(Process.GetCurrentProcess().MainModule?.FileName ?? string.Empty));
            builder.Append('}');
            return builder.ToString();
        }

        private static int Serve(string[] args, string outPath)
        {
            if (string.IsNullOrEmpty(outPath))
                return 2;

            var stopPath = Arg(args, "--stop") ?? outPath + ".stop";
            var parent = ArgInt(args, "--parent");
            SensorSession session = null;
            var tick = 0;

            try
            {
                while (true)
                {
                    if (parent > 0 && !ProcessExists(parent))
                        break;
                    if (File.Exists(stopPath))
                    {
                        TryDelete(stopPath);
                        break;
                    }

                    string json;
                    try
                    {
                        if (session == null)
                            session = new SensorSession();
                        json = SnapshotJson(session.Read(tick++), parent, null);
                    }
                    catch (Exception exception)
                    {
                        json = SnapshotJson(new List<Reading>(), parent, exception.Message);
                        try { session?.Dispose(); } catch { }
                        session = null;
                    }

                    WriteAtomic(outPath, json);
                    Thread.Sleep(1000);
                }
            }
            finally
            {
                try { session?.Dispose(); } catch { }
            }

            return 0;
        }

        private static int InstallPawnIo(string[] args, string outPath)
        {
            var installOut = Arg(args, "--install-out") ?? outPath;
            var setup = Arg(args, "--setup") ?? Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "PawnIO_setup.exe");
            if (!IsAdmin())
                return Write(installOut, InstallResultJson(false, -1, false, "Нужны права администратора"));
            if (!File.Exists(setup))
                return Write(installOut, InstallResultJson(false, -1, false, "PawnIO_setup.exe не найден рядом с модулем датчиков"));

            using (var process = Process.Start(new ProcessStartInfo(setup, "-install -silent")
            {
                UseShellExecute = false,
                CreateNoWindow = true,
            }))
            {
                if (process == null)
                    return Write(installOut, InstallResultJson(false, -1, false, "Не удалось запустить установщик PawnIO"));
                if (!process.WaitForExit(300000))
                {
                    try { process.Kill(); } catch { }
                    return Write(installOut, InstallResultJson(false, -1, false, "Установка PawnIO не завершилась за 5 минут"));
                }

                var code = process.ExitCode;
                var ok = code == 0 || code == 3010 || code == 1641;
                var reboot = code == 3010 || code == 1641;
                var error = ok ? null : "Установщик PawnIO вернул код " + code;
                return Write(installOut, InstallResultJson(ok, code, reboot, error));
            }
        }

        /* ------------------------------------------------------------------ */
        /*                              readings                              */
        /* ------------------------------------------------------------------ */

        private static string ReadOnceJson(int parent)
        {
            var readings = new List<Reading>();
            string error = null;
            try
            {
                using (var session = new SensorSession())
                    readings = session.Read(0);
            }
            catch (Exception exception)
            {
                error = exception.Message;
            }

            return SnapshotJson(readings, parent, error);
        }

        private static string SnapshotJson(List<Reading> readings, int parent, string error)
        {
            var builder = new StringBuilder();
            builder.Append('{');
            builder.Append("\"ok\":").Append(error == null ? "true" : "false").Append(',');
            builder.Append("\"generatedAt\":").Append(JsonString(DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture))).Append(',');
            builder.Append("\"pid\":").Append(Process.GetCurrentProcess().Id).Append(',');
            builder.Append("\"parent\":").Append(parent).Append(',');
            builder.Append("\"admin\":").Append(IsAdmin() ? "true" : "false").Append(',');
            builder.Append("\"pawnio\":").Append(PawnIo.IsInstalled ? "true" : "false").Append(',');
            builder.Append("\"sensors\":[");
            for (var index = 0; index < readings.Count; index++)
            {
                if (index > 0) builder.Append(',');
                builder.Append(readings[index].ToJson());
            }
            builder.Append("],");
            builder.Append("\"error\":").Append(error == null ? "null" : JsonString(error));
            builder.Append('}');
            return builder.ToString();
        }

        /// <summary>
        /// Keeps the LibreHardwareMonitor session open between snapshots. Opening
        /// it on every tick was expensive and added a visible lag.
        /// </summary>
        private sealed class SensorSession : IDisposable
        {
            private readonly Computer _computer;

            public SensorSession()
            {
                _computer = new Computer
                {
                    IsCpuEnabled = true,
                    IsGpuEnabled = true,
                    IsMotherboardEnabled = true,
                    IsStorageEnabled = true,
                    IsMemoryEnabled = true,
                    IsControllerEnabled = true,
                };
                _computer.Open();
                UpdateAll(_computer, 0);
                // The GPU/ADL providers need a second pass.
                Thread.Sleep(400);
                UpdateAll(_computer, 0);
            }

            public List<Reading> Read(int tick)
            {
                UpdateAll(_computer, tick);
                var readings = new List<Reading>();
                foreach (var hardware in _computer.Hardware)
                    Collect(hardware, readings, 0);

                var amd = TryAmdSdk(readings);
                if (amd != null)
                    readings.Add(amd);

                KeepPrimaryDiskTemperature(readings);

                return readings
                    .OrderBy(reading => reading.Kind == "temperature" ? 0 : 1)
                    .ThenBy(reading => reading.Hardware)
                    .ThenBy(reading => reading.Name)
                    .ToList();
            }

            public void Dispose()
            {
                try { _computer.Close(); } catch { }
            }
        }

        /// <summary>
        /// A drive often exposes a composite (controller) value and several
        /// NAND sensors. Users need one clear number, so keep the composite and
        /// drop the extra sensors; if there is no composite, promote the first.
        /// </summary>
        private static void KeepPrimaryDiskTemperature(List<Reading> readings)
        {
            var groups = readings
                .Where(reading => reading.Kind == "temperature" && reading.HardwareType == "Storage")
                .GroupBy(reading => reading.Hardware)
                .ToList();

            foreach (var group in groups)
            {
                var items = group.ToList();
                var main = items.FirstOrDefault(reading => reading.Name == "Температура накопителя");
                if (main == null && items.Count > 0)
                {
                    main = items[0];
                    main.Name = "Температура накопителя";
                }
                if (main == null)
                    continue;

                foreach (var extra in items)
                {
                    if (!ReferenceEquals(extra, main))
                        readings.Remove(extra);
                }
            }
        }

        private static void UpdateAll(Computer computer, int tick)
        {
            foreach (var hardware in computer.Hardware)
                UpdateAll(hardware, 0, tick);
        }

        private static void UpdateAll(IHardware hardware, int depth, int tick)
        {
            if (depth > 4)
                return;

            // SMART polling once a second is unnecessary; storage values change
            // slowly and the read can wake the drive.
            var refresh = hardware.HardwareType != HardwareType.Storage || tick % 5 == 0;
            if (refresh)
            {
                try { hardware.Update(); } catch { }
            }

            foreach (var sub in hardware.SubHardware)
                UpdateAll(sub, depth + 1, tick);
        }

        private static void Collect(IHardware hardware, List<Reading> readings, int depth)
        {
            if (depth > 4)
                return;

            foreach (var sub in hardware.SubHardware)
                Collect(sub, readings, depth + 1);

            foreach (var sensor in hardware.Sensors)
            {
                if (!sensor.Value.HasValue)
                    continue;
                var value = sensor.Value.Value;
                if (float.IsNaN(value) || float.IsInfinity(value))
                    continue;

                string kind = null;
                if (sensor.SensorType == SensorType.Temperature)
                {
                    // 0 °C means "driver/permission unavailable", not a real
                    // reading; do not show it as a value.
                    if (value <= 0 || IsThreshold(sensor.Name))
                        continue;
                    kind = "temperature";
                }
                else if (sensor.SensorType == SensorType.Fan)
                    kind = "fan";
                else if (sensor.SensorType == SensorType.Control && sensor.Name.IndexOf("fan", StringComparison.OrdinalIgnoreCase) >= 0)
                    kind = "percent";
                if (kind == null)
                    continue;

                readings.Add(new Reading
                {
                    Kind = kind,
                    Name = ReadableName(sensor.Name, sensor.SensorType),
                    Hardware = hardware.Name,
                    HardwareType = hardware.HardwareType.ToString(),
                    Value = Math.Round(value, 1),
                    Identifier = sensor.Identifier.ToString(),
                });
            }
        }

        private static string ReadableName(string name, SensorType type)
        {
            var lower = name.ToLowerInvariant();

            if (type == SensorType.Fan)
            {
                if (lower.Contains("cpu")) return "Вентилятор процессора";
                if (lower.Contains("gpu")) return "Вентилятор видеокарты";
                if (lower.Contains("system") || lower.Contains("case")) return "Корпусной вентилятор";
                if (lower.StartsWith("fan #")) return "Вентилятор " + name.Substring(6).Trim();
                return "Вентилятор";
            }

            if (type == SensorType.Control)
                return "Обороты вентилятора";

            if (lower.Contains("tctl") || lower.Contains("tdie")) return "Температура процессора";
            if (lower == "package") return "Температура процессора";
            if (lower == "core max") return "Максимум по ядрам";
            if (lower == "core average") return "Среднее по ядрам";
            if (lower.StartsWith("core #")) return "Ядро " + name.Substring(6).Trim();
            if (lower.StartsWith("ccd")) return "Кристалл";
            if (lower.Contains("composite")) return "Температура накопителя";
            if (lower.StartsWith("temperature #")) return "Датчик " + name.Substring(12).Trim().TrimStart('#').Trim();
            if (lower == "temperature") return "Температура";
            if (lower.Contains("hot spot")) return "Горячая точка";
            if (lower.Contains("memory junction")) return "Память видеокарты";
            if (lower == "gpu core") return "Ядро видеокарты";
            return name;
        }

        private static bool IsThreshold(string name)
        {
            if (string.IsNullOrEmpty(name))
                return false;
            foreach (var token in new[] { "critical", "warning", "limit", "max", "trip" })
            {
                if (name.IndexOf(token, StringComparison.OrdinalIgnoreCase) >= 0)
                    return true;
            }
            return false;
        }

        /* ------------------------------------------------------------------ */
        /*                          AMD Ryzen Master SDK                      */
        /* ------------------------------------------------------------------ */

        private static Reading TryAmdSdk(List<Reading> readings)
        {
            if (readings.Any(reading =>
                reading.Kind == "temperature" &&
                reading.HardwareType.IndexOf("Cpu", StringComparison.OrdinalIgnoreCase) >= 0))
                return null;

            var path = FindAmdSdk();
            if (path == null)
                return null;

            var module = LoadLibrary(path);
            if (module == IntPtr.Zero)
                return null;

            try
            {
                var initPtr = GetProcAddress(module, "PlatformInit");
                var queryPtr = GetProcAddress(module, "ShortQuery");
                var uninitPtr = GetProcAddress(module, "PlatformUninit");
                if (initPtr == IntPtr.Zero || queryPtr == IntPtr.Zero)
                    return null;

                var init = Marshal.GetDelegateForFunctionPointer<PlatformInitDelegate>(initPtr);
                var query = Marshal.GetDelegateForFunctionPointer<ShortQueryDelegate>(queryPtr);
                var uninit = uninitPtr == IntPtr.Zero
                    ? null
                    : Marshal.GetDelegateForFunctionPointer<PlatformUninitDelegate>(uninitPtr);

                if (!init())
                    return null;

                try
                {
                    var stats = query();
                    if (stats.Init == 0 || stats.Temperature <= 0 || stats.Temperature > 150)
                        return null;

                    return new Reading
                    {
                        Kind = "temperature",
                        Name = "Температура процессора",
                        Hardware = "AMD Ryzen",
                        HardwareType = "Cpu",
                        Value = Math.Round(stats.Temperature, 1),
                        Identifier = "/amdryzenmaster/0/temperature/0",
                    };
                }
                finally
                {
                    try { uninit?.Invoke(); } catch { }
                }
            }
            catch
            {
                return null;
            }
            finally
            {
                FreeLibrary(module);
            }
        }

        private static string FindAmdSdk()
        {
            var candidates = new List<string>();
            foreach (var root in new[]
                     {
                         Environment.GetFolderPath(Environment.SpecialFolder.ProgramFiles),
                         Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86),
                         AppDomain.CurrentDomain.BaseDirectory,
                     })
            {
                if (string.IsNullOrEmpty(root))
                    continue;
                candidates.Add(Path.Combine(root, "AMD", "RyzenMaster", "AMDRyzenMasterMonitoringDLL.dll"));
                candidates.Add(Path.Combine(root, "AMD", "RyzenMasterSDK", "AMDRyzenMasterMonitoringDLL.dll"));
                candidates.Add(Path.Combine(root, "AMD", "Performance Profile Client", "AMDRyzenMasterMonitoringDLL.dll"));
                candidates.Add(Path.Combine(root, "AMD", "CNext", "CNext", "AMDRyzenMasterMonitoringDLL.dll"));
                candidates.Add(Path.Combine(root, "AMDRyzenMasterMonitoringDLL.dll"));
            }

            return candidates.FirstOrDefault(File.Exists);
        }

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate bool PlatformInitDelegate();

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate bool PlatformUninitDelegate();

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        private delegate RmQuickStats ShortQueryDelegate();

        [StructLayout(LayoutKind.Sequential)]
        private struct RmQuickStats
        {
            public int Init;
            public float PptValue;
            public float EdcValue;
            public float TdcValue;
            public double Temperature;
            public double PeakCoreVoltage;
            public double AvgCoreVoltage;
        }

        /* ------------------------------------------------------------------ */
        /*                               plumbing                             */
        /* ------------------------------------------------------------------ */

        private sealed class Reading
        {
            public string Kind;
            public string Name;
            public string Hardware;
            public string HardwareType;
            public double Value;
            public string Identifier;

            public string ToJson()
            {
                return "{" +
                       "\"kind\":" + JsonString(Kind) + "," +
                       "\"name\":" + JsonString(Name) + "," +
                       "\"hardware\":" + JsonString(Hardware) + "," +
                       "\"hardwareType\":" + JsonString(HardwareType) + "," +
                       "\"value\":" + Value.ToString("0.0", CultureInfo.InvariantCulture) + "," +
                       "\"identifier\":" + JsonString(Identifier) +
                       "}";
            }
        }

        private static string InstallResultJson(bool ok, int exitCode, bool reboot, string error)
        {
            var builder = new StringBuilder();
            builder.Append('{');
            builder.Append("\"ok\":").Append(ok ? "true" : "false").Append(',');
            builder.Append("\"exitCode\":").Append(exitCode).Append(',');
            builder.Append("\"reboot\":").Append(reboot ? "true" : "false").Append(',');
            builder.Append("\"pawnioInstalled\":").Append(PawnIo.IsInstalled ? "true" : "false").Append(',');
            builder.Append("\"pawnioVersion\":").Append(PawnIo.Version == null ? "null" : JsonString(PawnIo.Version.ToString())).Append(',');
            builder.Append("\"error\":").Append(error == null ? "null" : JsonString(error));
            builder.Append('}');
            return builder.ToString();
        }

        private static string ErrorJson(string error)
        {
            return "{\"ok\":false,\"error\":" + JsonString(error) + "}";
        }

        private static int Write(string path, string json)
        {
            if (string.IsNullOrEmpty(path))
                return 2;
            try
            {
                WriteAtomic(path, json);
                return 0;
            }
            catch
            {
                return 1;
            }
        }

        private static void WriteAtomic(string path, string json)
        {
            var directory = Path.GetDirectoryName(path);
            if (!string.IsNullOrEmpty(directory))
                Directory.CreateDirectory(directory);

            var temporary = path + "." + Guid.NewGuid().ToString("N") + ".tmp";
            File.WriteAllText(temporary, json, new UTF8Encoding(false));
            try
            {
                if (File.Exists(path))
                {
                    File.Replace(temporary, path, null);
                }
                else
                {
                    File.Move(temporary, path);
                }
            }
            catch
            {
                TryDelete(temporary);
                File.WriteAllText(path, json, new UTF8Encoding(false));
            }
        }

        private static void TryDelete(string path)
        {
            try { File.Delete(path); } catch { }
        }

        private static bool ProcessExists(int pid)
        {
            try
            {
                var process = Process.GetProcessById(pid);
                return !process.HasExited;
            }
            catch
            {
                return false;
            }
        }

        private static bool IsAdmin()
        {
            using (var identity = WindowsIdentity.GetCurrent())
                return new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator);
        }

        private static string Arg(string[] args, string name)
        {
            for (var index = 0; index < args.Length - 1; index++)
            {
                if (args[index] == name)
                    return args[index + 1];
            }
            return null;
        }

        private static int ArgInt(string[] args, string name)
        {
            int value;
            return int.TryParse(Arg(args, name), out value) ? value : 0;
        }

        private static string JsonString(string value)
        {
            if (value == null)
                return "null";

            var builder = new StringBuilder(value.Length + 2);
            builder.Append('"');
            foreach (var character in value)
            {
                switch (character)
                {
                    case '"': builder.Append("\\\""); break;
                    case '\\': builder.Append("\\\\"); break;
                    case '\n': builder.Append("\\n"); break;
                    case '\r': builder.Append("\\r"); break;
                    case '\t': builder.Append("\\t"); break;
                    default:
                        if (character < 0x20)
                            builder.Append("\\u").Append(((int)character).ToString("x4"));
                        else
                            builder.Append(character);
                        break;
                }
            }
            builder.Append('"');
            return builder.ToString();
        }

        /* ------------------------------------------------------------------ */
        /*                               win32                                */
        /* ------------------------------------------------------------------ */

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr LoadLibrary(string path);

        [DllImport("kernel32.dll", CharSet = CharSet.Ansi, SetLastError = true)]
        private static extern IntPtr GetProcAddress(IntPtr module, string name);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool FreeLibrary(IntPtr module);
    }
}
