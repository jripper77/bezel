// Bezel sensor helper: GPL-3.0-or-later. LibreHardwareMonitorLib is MPL-2.0.
using System;
using System.Collections.Generic;
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
        bool created;
        using (Mutex mutex = new Mutex(true, "Local\\BezelSensors-" + identity.User.Value, out created))
        {
            if (!created) return 0;
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
                computer.Open();
                Log("Started LibreHardwareMonitorLib " + typeof(Computer).Assembly.GetName().Version);
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
                    catch (Exception error) { Log("Sample failed: " + error.GetType().Name + ": " + error.Message); }
                    if (once) break;
                    DateTime requested = File.Exists(heartbeat) ? File.GetLastWriteTimeUtc(heartbeat) : started;
                    if (DateTime.UtcNow - requested > TimeSpan.FromSeconds(30))
                    {
                        Log("No Bezel clients; stopping");
                        break;
                    }
                    Thread.Sleep(1000);
                } while (true);
                return File.Exists(output) ? 0 : 4;
            }
            catch (Exception error) { Log("Startup failed: " + error.GetType().Name + ": " + error.Message); return 1; }
            finally
            {
                try { if (computer != null) computer.Close(); } catch (Exception error) { Log("Cleanup failed: " + error.GetType().Name); }
                mutex.ReleaseMutex();
            }
        }
    }

    private static object ReadHardware(IHardware hardware)
    {
        hardware.Update();
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
                File.WriteAllText(logPath, String.Empty);
            File.AppendAllText(logPath, DateTime.UtcNow.ToString("o") + " " + message + Environment.NewLine);
        }
        catch (IOException) { }
    }

    private static int SelfTest()
    {
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
