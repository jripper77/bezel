// Bezel sensor helper: GPL-3.0-or-later. LibreHardwareMonitorLib is MPL-2.0.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Security.Principal;
using System.Threading;
using System.Web.Script.Serialization;
using System.Xml;
using LibreHardwareMonitor.Hardware;

internal static class Program
{
    private static string logPath;
    private static readonly Dictionary<string, string> hardwareHealth = new Dictionary<string, string>();
    private static readonly Dictionary<string, DateTime> hardwareLoggedAt = new Dictionary<string, DateTime>();

    private static int Main(string[] args)
    {
        string folder = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "io.github.slipalison.bezel", "sensors");
        Directory.CreateDirectory(folder);
        logPath = Path.Combine(folder, "helper.log");
        string output = Path.Combine(folder, "hardware.json");
        string names = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "sensor-names.xml");
        bool once = false;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--once") once = true;
            else if (args[i] == "--output" && i + 1 < args.Length) output = Path.GetFullPath(args[++i]);
            else if (args[i] == "--names" && i + 1 < args.Length) names = Path.GetFullPath(args[++i]);
            else if (args[i] == "--self-test") return SelfTest();
            else { Log("Unknown argument"); return 2; }
        }
        WindowsIdentity identity = WindowsIdentity.GetCurrent();
        bool elevated = new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator);
        if (!elevated) { Log("Administrator rights required for hardware sensors"); return 3; }
        using (Mutex mutex = new Mutex(false, "Local\\BezelSensors-" + identity.User.Value))
        {
            if (!TryAcquire(mutex)) { Log("Another sensor helper instance owns the reader; exiting"); return 0; }
            Computer computer = null;
            try
            {
                computer = new Computer(new NameSettings(names));
                computer.IsCpuEnabled = true;
                computer.IsGpuEnabled = true;
                computer.IsMotherboardEnabled = true;
                computer.IsControllerEnabled = true;
                computer.IsPsuEnabled = true;
                computer.IsPowerMonitorEnabled = true;
                computer.IsMemoryEnabled = true;
                computer.IsStorageEnabled = true;
                computer.IsNetworkEnabled = true;
                computer.IsBatteryEnabled = true;
                Log("Opening hardware; pid=" + Process.GetCurrentProcess().Id + "; helper=" + typeof(Program).Assembly.Location + "; library=" + typeof(Computer).Assembly.GetName().Version);
                Stopwatch opening = Stopwatch.StartNew();
                computer.Open();
                Log("Started LibreHardwareMonitorLib " + typeof(Computer).Assembly.GetName().Version + "; openMs=" + opening.ElapsedMilliseconds + "; hardwareCount=" + computer.Hardware.Count);
                DateTime started = DateTime.UtcNow;
                string heartbeat = Path.Combine(folder, "request");
                JavaScriptSerializer json = new JavaScriptSerializer();
                json.MaxJsonLength = 4 * 1024 * 1024;
                do
                {
                    try
                    {
                        List<object> children = new List<object>();
                        foreach (IHardware hardware in computer.Hardware) children.Add(ReadHardware(hardware));
                        Dictionary<string, object> snapshot = new Dictionary<string, object>();
                        snapshot["Provider"] = "Bezel LibreHardwareMonitorLib";
                        snapshot["UpdatedUtc"] = DateTime.UtcNow.ToString("o", CultureInfo.InvariantCulture);
                        snapshot["Elevated"] = elevated;
                        snapshot["Children"] = children;
                        Publish(output, json.Serialize(snapshot));
                    }
                    catch (Exception error) { Log("Sample failed: " + error.ToString()); }
                    if (once) break;
                    DateTime requested = File.Exists(heartbeat) ? File.GetLastWriteTimeUtc(heartbeat) : started;
                    if (ShouldStop(started, requested, DateTime.UtcNow))
                    {
                        Log("No Bezel clients; stopping; heartbeatAgeSeconds=" + (DateTime.UtcNow - requested).TotalSeconds.ToString("F1", CultureInfo.InvariantCulture));
                        break;
                    }
                    Thread.Sleep(1000);
                } while (true);
                return File.Exists(output) ? 0 : 4;
            }
            catch (Exception error) { Log("Startup failed: " + error.ToString()); return 1; }
            finally
            {
                try { if (computer != null) computer.Close(); } catch (Exception error) { Log("Cleanup failed: " + error.ToString()); }
                Log("Helper stopped; pid=" + Process.GetCurrentProcess().Id);
                mutex.ReleaseMutex();
            }
        }
    }

    // A named mutex can survive its owner (other handles and task teardown).
    // Existence is not ownership. An abandoned mutex is acquired by WaitOne.
    private static bool TryAcquire(Mutex mutex)
    {
        try { return mutex.WaitOne(0); }
        catch (AbandonedMutexException) { Log("Acquired abandoned reader mutex"); return true; }
    }

    // A heartbeat from a previous session must not cancel the startup grace period.
    private static bool ShouldStop(DateTime started, DateTime requested, DateTime now)
    {
        return now - (requested > started ? requested : started) > TimeSpan.FromSeconds(30);
    }

    private static object ReadHardware(IHardware hardware)
    {
        Stopwatch update = Stopwatch.StartNew();
        try { hardware.Update(); }
        catch (Exception error) { Log("Hardware update failed; id=" + hardware.Identifier + "; elapsedMs=" + update.ElapsedMilliseconds + "; " + error.ToString()); throw; }
        LogHardware(hardware, update.ElapsedMilliseconds);
        List<object> children = new List<object>();
        foreach (ISensor sensor in hardware.Sensors)
        {
            Dictionary<string, object> node = new Dictionary<string, object>();
            node["Text"] = sensor.Name;
            node["SensorId"] = sensor.Identifier.ToString();
            node["Type"] = sensor.SensorType.ToString();
            node["RawValue"] = sensor.Value.HasValue && !float.IsNaN(sensor.Value.Value) && !float.IsInfinity(sensor.Value.Value)
                ? sensor.Value.Value.ToString("R", CultureInfo.InvariantCulture) : null;
            children.Add(node);
        }
        foreach (IHardware sub in hardware.SubHardware) children.Add(ReadHardware(sub));
        Dictionary<string, object> result = new Dictionary<string, object>();
        result["Text"] = hardware.Name;
        result["HardwareId"] = hardware.Identifier.ToString();
        result["Children"] = children;
        return result;
    }

    private static void Publish(string output, string json)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(output));
        string temporary = output + ".tmp";
        File.WriteAllText(temporary, json, new System.Text.UTF8Encoding(false));
        // Replace atomically: readers never observe a partially written snapshot.
        if (File.Exists(output)) File.Replace(temporary, output, null);
        else File.Move(temporary, output);
    }

    private static void Log(string message)
    {
        try
        {
            if (File.Exists(logPath) && new FileInfo(logPath).Length > 1024 * 1024)
            {
                string previous = logPath + ".1";
                if (File.Exists(previous)) File.Delete(previous);
                File.Move(logPath, previous);
            }
            File.AppendAllText(logPath, DateTime.UtcNow.ToString("o") + " pid=" + Process.GetCurrentProcess().Id + " " + message + Environment.NewLine);
        }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }

    private static bool HasValue(ISensor sensor)
    {
        return sensor.Value.HasValue && !float.IsNaN(sensor.Value.Value) && !float.IsInfinity(sensor.Value.Value);
    }

    private static void LogHardware(IHardware hardware, long elapsedMs)
    {
        string id = hardware.Identifier.ToString();
        int valid = 0;
        List<string> missing = new List<string>();
        List<string> values = new List<string>();
        foreach (ISensor sensor in hardware.Sensors)
        {
            if (HasValue(sensor)) valid++;
            else missing.Add(sensor.Identifier.ToString());
            if (id.StartsWith("/psu/corsair/", StringComparison.Ordinal))
                values.Add(sensor.Identifier + "=" + (HasValue(sensor) ? sensor.Value.Value.ToString("R", CultureInfo.InvariantCulture) : "missing"));
        }
        string state = "valid=" + valid + "/" + hardware.Sensors.Length + "; missing=[" + String.Join(",", missing.ToArray()) + "]";
        string previous;
        DateTime last;
        if (!hardwareHealth.TryGetValue(id, out previous) || previous != state ||
            !hardwareLoggedAt.TryGetValue(id, out last) || DateTime.UtcNow - last >= TimeSpan.FromSeconds(30))
        {
            Log("Hardware sample; id=" + id + "; updateMs=" + elapsedMs + "; " + state +
                (values.Count > 0 ? "; corsair=[" + String.Join(",", values.ToArray()) + "]" : ""));
            hardwareHealth[id] = state;
            hardwareLoggedAt[id] = DateTime.UtcNow;
        }
    }

    private static int SelfTest()
    {
        string testMutexName = "Local\\BezelSensorsTest-" + Guid.NewGuid().ToString("N");
        using (Mutex observer = new Mutex(false, testMutexName))
        {
            Thread owner = new Thread(delegate() {
                using (Mutex held = new Mutex(false, testMutexName)) { held.WaitOne(); }
                // Exit without releasing ownership, retaining observer's handle.
            });
            owner.Start(); owner.Join();
            if (!TryAcquire(observer)) return 1;
            bool rejected = false;
            Thread contender = new Thread(delegate() {
                using (Mutex held = new Mutex(false, testMutexName)) {
                    rejected = !TryAcquire(held);
                    if (!rejected) held.ReleaseMutex();
                }
            });
            contender.Start(); contender.Join();
            observer.ReleaseMutex();
            if (!rejected) return 1;
        }
        DateTime started = new DateTime(2026, 10, 6, 20, 0, 0, DateTimeKind.Utc);
        if (ShouldStop(started, started.AddDays(-1), started.AddSeconds(1))) return 1;
        if (ShouldStop(started, started, started.AddSeconds(30))) return 1;
        if (!ShouldStop(started, started.AddDays(-1), started.AddSeconds(31))) return 1;
        if (ShouldStop(started, started.AddSeconds(25), started.AddSeconds(40))) return 1;
        if (!ShouldStop(started, started.AddSeconds(25), started.AddSeconds(56))) return 1;
        JavaScriptSerializer serializer = new JavaScriptSerializer();
        Dictionary<string, object> sensor = new Dictionary<string, object>();
        sensor["SensorId"] = "/intelcpu/0/temperature/0";
        sensor["RawValue"] = 42.5f.ToString("R", CultureInfo.InvariantCulture);
        Dictionary<string, object> decoded = serializer.Deserialize<Dictionary<string, object>>(serializer.Serialize(sensor));
        if ((string)decoded["RawValue"] != "42.5") return 1;
        NameSettings settings = new NameSettings(null);
        settings.SetValue("/test/name", "CPU Fan");
        if (settings.GetValue("/test/name", "") != "CPU Fan") return 1;
        settings.SetValue("/test/control/mode", "2");
        if (settings.GetValue("/test/control/mode", "0") != "0") return 1;
        return 0;
    }

    // Import labels only. Never restore persisted software fan-control modes.
    private sealed class NameSettings : ISettings
    {
        private readonly Dictionary<string, string> values = new Dictionary<string, string>();
        internal NameSettings(string path)
        {
            if (path == null || !File.Exists(path)) return;
            XmlDocument document = new XmlDocument();
            document.XmlResolver = null;
            document.Load(path);
            foreach (XmlElement item in document.SelectNodes("//add"))
            {
                string key = item.GetAttribute("key");
                if (IsName(key)) values[key] = item.GetAttribute("value");
            }
        }
        private static bool IsName(string name) { return name.StartsWith("/", StringComparison.Ordinal) && name.EndsWith("/name", StringComparison.Ordinal); }
        public bool Contains(string name) { return IsName(name) && values.ContainsKey(name); }
        public string GetValue(string name, string fallback) { string value; return IsName(name) && values.TryGetValue(name, out value) ? value : fallback; }
        public void SetValue(string name, string value) { if (IsName(name)) values[name] = value; }
        public void Remove(string name) { values.Remove(name); }
    }
}
