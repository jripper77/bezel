# Sensors

Every metric either reference app offers, how each one measures it, the measurement bugs Bezel must not repeat, and
the strategy Bezel follows on Linux and Windows. Confidence: **static** (nothing here was measured on a machine);
items marked **inferred** rely on third-party library behaviour that was not executed.

## 1. Principles for Bezel

1. **Unavailable is a state, not a value.** A metric that cannot be read renders as unavailable (for example `--`)
   and recovers when the source returns. Never substitute 0, a random number, a stale value without a marker, or hide
   the widget forever.
2. **Rates use measured elapsed time** (monotonic clock), never the nominal polling interval. Counter resets and wraps
   skip a sample instead of producing negative or huge values.
3. **Units are explicit.** Binary (MiB/GiB) and decimal (MB/GB) are never mixed under one label.
4. **Sensors have stable ids** (for example `hwmon:k10temp/Tctl`, `nvml:0/temperature`), chosen from a live sensor
   browser. The Python theme keys and the vendor data names are aliases with default bindings.
5. **No vendor engines.** Bezel does not bundle the vendor's sensor engine or reuse any credentials the vendor app
   embeds.

## 2. Unified catalog

`PY` = turing-smart-screen-python (`library/sensors/sensors_python.py` on Linux, `sensors_librehardwaremonitor.py`
"LHM" on Windows). `TZ` = the vendor app (Windows only). A dash means the app does not offer the metric.

| Metric | PY theme key | TZ data name | PY Linux | PY Windows (LHM) | TZ source | Unit |
|---|---|---|---|---|---|---|
| CPU usage, total | `CPU.PERCENTAGE` | `CPULOAD` | `psutil.cpu_percent(interval=INTERVAL)`, blocking for INTERVAL | Load sensor starting `CPU Total` | engine usage sensor `Total CPU Utility`, else `Total CPU Usage` (user-selectable) | % |
| CPU usage, per core | - | - | - | - | - | % |
| CPU frequency | `CPU.FREQUENCY` | `CPUCLOCK` (MHz), `CPUCLOCK_G` (GHz) | `psutil.cpu_freq().current` (mean of `scaling_cur_freq`) | mean of Clock sensors containing `Core #` and not `Effective` | mean of per-core clock sensors (label contains `core`; E-cores handled separately) | MHz / GHz |
| CPU load average | `CPU.LOAD.ONE/FIVE/FIFTEEN` | - | `os.getloadavg()` | psutil emulation | - | run-queue length (shown as `%`, bug) |
| CPU temperature | `CPU.TEMPERATURE` | `CPUTEMP` | first entry of the first chip among `coretemp`, `k10temp`, `cpu_thermal`, `zenpower` | Temperature sensor starting `Core Average`, then `Core Max`, then `CPU Package`, then any `Core...` | engine temperature `CPU Package` by default (user-selectable, e.g. `SOC`); fallback max core temperature | °C (°F option) |
| CPU power | - | `CPUPWR` | - | - | engine power sensor `CPU Package Power` | W |
| CPU voltage | - | `CPUVOLTAGE` | - | - | per-core `VID` sensors averaged (`VID Average`, user-selectable) | V |
| CPU fan | `CPU.FAN_SPEED` | `CPUFAN` | hwmon heuristic percent (section 5, S3/S4) | first Control sensor containing `#2` of the Super-I/O (fan control %) | user-selected fan sensor, fuzzy match `cpu` | PY %, TZ RPM |
| CPU model | - | `CPUMODEL` | - | - | engine device name without `AMD`/`Intel`; manual override | text |
| GPU usage | `GPU.PERCENTAGE` | `GPULOAD` | NVIDIA: mean of GPUtil `load * 100` over all GPUs; AMD: `pyamdgpuinfo query_load() * 100` | Load `GPU Core`, fallback `D3D 3D` | first of `GPU Core Load`, `GPU Utilization`, `GPU Total Usage` | % |
| GPU memory % | `GPU.MEMORY_PERCENT` | `GPURAMLOAD` | used / total | used / total | `GPU Memory Usage` | % |
| GPU memory used | `GPU.MEMORY_USED` | `GPURAM` | NVIDIA `memoryUsed` (MiB); AMD `query_vram_usage()/1024/1024` | SmallData `GPU Memory Used`, fallback `D3D ... Memory Used` | `GPU D3D Memory Dedicated` | MiB / MB |
| GPU memory total | `GPU.MEMORY_TOTAL` | `GPURAMTOTAL` | `memoryTotal` / `vram_size` | SmallData `GPU Memory Total` | engine VRAM size (KB -> MB) | MiB / MB |
| GPU temperature | `GPU.TEMPERATURE` | `GPUTEMP` | NVIDIA mean temperature; AMD `query_temperature()` | Temperature `GPU Core` | first of `GPU Temperature`, `GPU Thermal Diode`, `GPU Core Temperature`, `GPU Global Temperature` | °C |
| GPU clock | `GPU.FREQUENCY` | `GPUCLOCK`, `GPUCLOCK_G` | NVIDIA: **NaN**; AMD `query_sclk()/1e6` | first Clock containing `Core`, not `Effective` | `GPU Clock` or `GPU Shader Clock` | MHz / GHz |
| GPU power | - | `GPUPWR` | - | - | label containing `Power`, excluding `Limit`/`Misc` | W |
| GPU fan | `GPU.FAN_SPEED` | `GPUFAN` | hwmon rule (bug S2); AMD then pyadl (Windows only) | first Control sensor of the GPU (%) | GPU device fan sensor | PY %, TZ RPM |
| GPU voltage | - | `GPUVOLTAGE` | - | - | `GPU Core Voltage` | V |
| GPU model | - | `GPUMODEL` | - | - | device name without vendor; manual override | text |
| Game FPS | `GPU.FPS` | `FPS` | -1 (unsupported) | Factor sensor with `FPS` in its name; keeps the previous value when `<= 0` | RTSS frame rate exposed by the engine as an "other" sensor `Framerate` | fps |
| RAM % | `MEMORY.VIRTUAL.*` / `PERCENT_TEXT` | `RAMLOAD` | `psutil.virtual_memory().percent` = (total - available) / total | Load starting `Memory` of "Total Memory" | `Physical Memory Load` | % |
| RAM used | `MEMORY.VIRTUAL.USED` | `RAM`, `RAM_GB` | `total - available` | Data `Memory Used` (GB) x 1e9 | `Physical Memory Used` | PY MiB, TZ MB / GB (= MB / 1024) |
| RAM available | `MEMORY.VIRTUAL.FREE` | `RAMVALID`, `RAMVALID_GB` | `available` | Data `Memory Available` x 1e9 | `Physical Memory Available` | as above |
| RAM total | `MEMORY.VIRTUAL.TOTAL` | `RAMTOTAL`, `RAMTOTAL_GB` | `available + used` | sum of the two | engine total physical RAM (MB) | as above |
| RAM "model" | - | `RAMMODEL` | - | - | `<total / 1024>G` (e.g. `16G`); manual override | text |
| Swap % | `MEMORY.SWAP` | - | `psutil.swap_memory().percent` | derived: ("Virtual Memory" - "Total Memory") used / (used + available) | - | % |
| Disk used / total / free | `DISK.USED/TOTAL/FREE` | `DRVLOAD` (%) | `psutil.disk_usage("/")` | `psutil.disk_usage("/")` (current drive) | .NET `DriveInfo`: `(1 - TotalFreeSpace / TotalSize) * 100` for the drive letter in SubName | %, decimal GB |
| Disk temperature | - | `HDDTEMP` | - | - | `Drive Temperature`, fallback `Drive Airflow Temperature` (disk index in SubName) | °C |
| Disk activity | - | `HDDUSED` | - | - | `Total Activity` per drive | % |
| Network rate up / down | `NET.WLO/ETH.UPLOAD/DOWNLOAD` | `UPSPEED`, `DOWNDSPEED` | psutil per-NIC byte counters / **nominal** INTERVAL | Throughput `Upload Speed`/`Download Speed`; psutil fallback with monotonic elapsed time | .NET `NetworkInterface.GetIPv4Statistics()` byte-counter delta / elapsed, for one selected adapter | B/s (PY), KB/s or MB/s (TZ) |
| Network totals | `NET.*.UPLOADED/DOWNLOADED` | - | cumulative since boot | Data `Data Uploaded/Downloaded` (GB) x 1e9 | - | bytes |
| Pump, case fans | - | `WATERPUMP`, `CASEFAN1`, `CASEFAN2` | - | - | user-selected fan sensors (fuzzy `pump` for the pump) | RPM |
| Date / time | `DATE.DAY/HOUR` | `TIME`, `DATE`, `DAY`, `APM` | Babel with the process locale | Babel with `locale.getdefaultlocale()` | `DateTime.Now` formatted by SubName | text |
| Uptime | `UPTIME.SECONDS/FORMATTED` | - | `uptime.uptime()` | same | - | s |
| Ping | `PING` | - | `ping3.ping(host, unit="ms")`, 4 s timeout | same | - | ms |
| Weather | `WEATHER.*` | `Weather` | OpenWeatherMap One Call 3.0 (user key) | same | QWeather HTTP API with the vendor's own credentials | text |
| System volume | - | `Volume` | - | - | NAudio default render endpoint master volume scalar x 100 | % |
| Custom | `CUSTOM.<Class>` | - | user Python class | same | - | any |
| Static text | `static_text` | `StaticText` | - | - | - | text |

Not offered by either app: per-core usage or clocks, disk I/O throughput, RAM or motherboard temperatures, battery,
GPU hot-spot or memory clock, multi-GPU selection (PY), aggregated network, process lists.

## 3. Vendor data names (alias table)

Order = the editor's data picker. Unit letters come from the value formatter: `%`, `°` (with `Fahrenheit`:
`32 + c * 1.8`), `M` (MHz or MB), `G` (GB), `R` (RPM), `W`, `V`, `KB/s` (shown as `MB/s` above 1024), nothing for FPS,
model names and time. `ShowUnit` toggles the suffix.

| # | DataName | Meaning | Format |
|---|---|---|---|
| 0 | `StaticText` | literal text | text |
| 1 | `CPUTEMP` | CPU temperature | integer °C / °F |
| 2 | `CPUCLOCK` | CPU clock | integer MHz |
| 3 | `CPUCLOCK_G` | CPU clock | `0.00` GHz |
| 4 | `CPULOAD` | CPU usage | % |
| 5 | `CPUPWR` | CPU package power | W |
| 6 | `CPUFAN` | CPU fan | RPM |
| 7 | `CPUVOLTAGE` | CPU core voltage | `0.00` V |
| 8 | `CPUMODEL` | CPU name | text |
| 9 | `GPUTEMP` | GPU temperature | °C |
| 10 | `GPUCLOCK` | GPU core clock | MHz |
| 11 | `GPUCLOCK_G` | GPU core clock | `0.00` GHz |
| 12 | `GPURAMTOTAL` | VRAM total | MB |
| 13 | `GPURAMLOAD` | VRAM usage | % |
| 14 | `GPURAM` | VRAM used | MB |
| 15 | `GPULOAD` | GPU usage | % |
| 16 | `GPUPWR` | GPU power | W |
| 17 | `GPUFAN` | GPU fan | RPM |
| 18 | `GPUVOLTAGE` | GPU voltage | `0.00` V |
| 19 | `GPUMODEL` | GPU name | text |
| 20 | `RAMVALID` | RAM available | MB |
| 21 | `RAMLOAD` | RAM usage | % |
| 22 | `RAM` | RAM used | MB |
| 23 | `RAMTOTAL` | RAM total | MB |
| 24 | `RAMMODEL` | memory "model" | text |
| 25 | `RAM_GB` | RAM used | `0.0` GB |
| 26 | `RAMVALID_GB` | RAM available | `0.0` GB |
| 27 | `RAMTOTAL_GB` | RAM total | `0.0` GB |
| 28 | `WATERPUMP` | AIO pump | RPM |
| 29 | `CASEFAN1` | case fan 1 | RPM |
| 30 | `CASEFAN2` | case fan 2 | RPM |
| 31 | `DRVLOAD` | drive space used (SubName = drive letter) | integer % |
| 32 | `HDDTEMP` | disk temperature (SubName = disk index) | °C |
| 33 | `HDDUSED` | disk activity (SubName = disk index) | % |
| 34 | `UPSPEED` | network upload | KB/s |
| 35 | `DOWNDSPEED` | network download | KB/s |
| 36 | `TIME` | clock (SubName = format) | text |
| 37 | `DATE` | date (SubName = format) | text |
| 38 | `DAY` | weekday (SubName = format) | text |
| 39 | `APM` | `AM` / `PM` | text |
| 40 | `Volume` | system master volume | % |
| 41 | `Weather` | weather string `"<condition> <wind><scale>  <temp>°"` | text |
| 42 | `FPS` | game frame rate | integer |

`TIME` SubName formats: `hm` (`h:mm`), `HHmm`, `h_12`, `h_24`, `hh`, `HH`, `mm`, `ss`. `DATE`: `Y-M-D`/`yyyy-MM-dd`,
`yyyy`, `M_en` (`MMM` en-US), `M_cn` (`MMM` zh-CN), `MM`, `dd`. `DAY`: `Day_cn` (`ddd` zh-CN), `Day_en` (`ddd`),
`Num_cn`, `Num`. Editor preview values: CPUCLOCK 1888, CPUTEMP 88, RAMLOAD 88, FPS 88, RAMMODEL `DDR4-16GB`, CPUMODEL
`I9 10900K`, GPUMODEL `RTX3080TI`, DATE `2020-12-31`, DAY `Sat`/`周一`.

## 4. How the references measure

### 4.1 Python (`library/stats.py`, `library/sensors/`)

- Backend selection by `config.yaml HW_SENSORS`: `PYTHON` (psutil, GPUtil, pyamdgpuinfo, pyadl), `LHM` (Windows only;
  elsewhere it logs an error and exits with code 0), `STUB` (seeded random values), `STATIC` (fixed values: 50 %,
  67.3 °C, 2400 MHz CPU, 1500 MHz GPU, 120 FPS, 1000 GB disk, 64 GB RAM, 32 GB VRAM, 1,061,000,000 B network; used for
  previews), `AUTO` (default: Windows -> LHM, others -> PYTHON).
- One scheduler thread per metric group at the theme's `INTERVAL`; values go through `int()` truncation, `MIN_SIZE`
  padding and unit suffixes ([themes-python-yaml.md](themes-python-yaml.md) section 8).
- GPU detection on Linux: NVIDIA (GPUtil, which runs `nvidia-smi` as a subprocess on every call) then AMD
  (pyamdgpuinfo, then pyadl). **Intel GPUs on Linux are not supported.** NVIDIA values are averaged over all GPUs; AMD
  uses GPU 0. The GPU thread is not started when no GPU is found.
- LHM GPU choice (Windows): no GPU -> disabled; one -> it; several -> first NVIDIA, else the single AMD, else the AMD
  that has a `GPU Core` load sensor (skipping an APU), else the first Intel. Chosen once at startup by name; not
  configurable.
- CPU temperature on Linux reads `psutil.sensors_temperatures()` (hwmon and thermal zones): for `coretemp` the first
  entry is typically `Package id 0`; for `k10temp` it is `Tctl` (or `Tdie` on some kernels; `Tctl` carries a +10/+20/
  +27 °C offset on some older Ryzen and Threadripper parts).
- Fans on Linux: a private copy of psutil's fan reader globs `/sys/class/hwmon/hwmon*/fan*_*` (fallback
  `.../device/fan*_*`); per fan: `current = fanN_input`; `max = fanN_max`, else a guess by current RPM (> 2200 -> 3000,
  > 1500 -> 2200, else 1500); `min = fanN_min`, else 0; **percent = `int((cur - min) / (max - min) * 100)`**. Grouped by
  the chip `name`, label `fanN_label` or `fanN`. Auto-selection: first fan whose label or chip contains `cpu` or
  `proc`; manual: `CPU_FAN: "<chip>/<label>"` (for example `nct6798/fan2`). The Windows LHM path ignores `CPU_FAN`.
- Disk: `psutil.disk_usage("/")` on both systems; `TOTAL = free + used` (excludes reserved blocks).
- Network (PY): `psutil.net_io_counters(pernic=True)`; rate = `(now - prev) / INTERVAL`. (LHM): Throughput sensors;
  when absent (some Wi-Fi adapters) psutil with monotonic elapsed time and `max(0, delta)`.
- Date: Babel `format_date`/`format_time` with `FORMAT`; locale from `locale.getdefaultlocale()` on Windows and
  `babel.dates.LC_TIME` elsewhere, fallback `en_US`.
- Ping: `ping3` (ICMP; unprivileged datagram sockets on Linux need `net.ipv4.ping_group_range`), synchronous.
- Weather: `GET https://api.openweathermap.org/data/3.0/onecall?lat=..&lon=..&exclude=minutely,hourly,daily,alerts&appid=..&units=..&lang=..`
  with no timeout; period `max(300, INTERVAL)` (at most 288 calls/day).
- Custom: every key under `STATS.CUSTOM` (except `INTERVAL`) names a class in `sensors_custom.py`, instantiated on
  every tick, with `as_numeric()`, `as_string()`, `last_values()`. An exception in one class aborts the whole custom
  loop for that tick.

### 4.2 Vendor app

- **Engine**: all hardware readings come from a bundled copy of the HWiNFO sensor engine, shipped under a generic
  runtime-library file name and called by export ordinal (initialisation, device enumeration, device names, VRAM
  size, total RAM, and typed reads for temperature, clock, voltage, power, fan RPM, usage, current and "other" sensors
  with a unit string). The integer in `code.ini` (default `64`) is passed to the engine's init
  ([runtime-artifacts.md](runtime-artifacts.md) section 4). The app requires administrator rights for it.
- **Sensor selection is by label string**: the engine's labels are matched with fixed lists (section 2), and nine
  settings let the user pick a label from drop-downs that show the raw engine labels (network adapter, CPU temperature,
  CPU voltage, CPU usage, GPU, CPU fan, pump, case fan 1, case fan 2). Fan picks are stored as `"<device index>.<label>"`.
  The choices are stored as label strings, so they break when labels are localised or renamed.
- **GPU**: devices whose name contains `GPU`, skipping names containing `ASUS EC`; the selected `GPU [#n]: <name>`
  entry (default the first) gives the device index.
- **Polling**: one thread refreshes each engine device (then sleeps 20 ms); disk devices are refreshed with 10 ms
  sleeps and only when a longer period has elapsed. Values are assigned to the theme's elements on **every rendered
  frame** (theme frame rate, default 20 fps), including time, date, volume and network.
- **FPS**: requires RivaTuner Statistics Server (bundled with MSI Afterburner) running; RTSS publishes the frame rate
  in its shared memory, the engine exposes it as an "other" sensor `Framerate`, and the app takes `(int)value`. The
  settings page has an "FPS (RTSS)" checkbox and a "Detect RTSS" button that launches `Plugin\RivaTuner Statistics
  Server\RTSS.exe` if present.
- **Disk space**: .NET `DriveInfo` per drive letter. **Network**: .NET `NetworkInterface` statistics of one selected
  adapter (default: the first), delta divided by elapsed time, displayed in KB/s. **Volume**: NAudio default render
  endpoint. **Weather**: QWeather city lookup and "now" endpoints, city stored in the settings, result cached (refreshed
  after 12 h), text in Chinese or English.

## 5. Measurement bugs in the references (Bezel must not repeat them)

Python (all confirmed from source unless marked):

| # | Where | Bug |
|---|---|---|
| S1 | `stats.py:307-314`, `sensors_python.py:138-142` | Load average (unnormalised run-queue length) is printed as an integer with `%` |
| S2 | `sensors_python.py:294, 382` | GPU fan match `"gpu" in (label.lower() or name.lower())`: the label is never empty, so the chip name is never tested (an `amdgpu` chip with label `fan1` never matches) |
| S3 | `sensors_python.py:106` | Fan percent can exceed 100 or go negative (guessed max); `max == min` raises ZeroDivisionError outside the per-fan handler, so the caller returns NaN for all fans |
| S4 | `sensors_python.py:171-178` | Manual `CPU_FAN` wins only if no earlier fan matches the automatic `cpu`/`proc` rule |
| S5 | `stats.py:298-303, 591-596` | CPU/GPU frequency radials receive a **string** (`f"{ghz:.2f}"`): TypeError, the thread dies |
| S6 | `stats.py:286-291, 391` | Frequency has no NaN handling: unsupported (always on NVIDIA Linux) prints `nan GHz`; the bar gets `int(GHz)` (1 GHz steps) |
| S7 | `stats.py:269-281` | CPU% NaN -> `int(nan)` ValueError, the thread dies |
| S8 | `stats.py:935-954` | Ping timeout returns `None`, error `False`; `int(None)` kills the thread; the call blocks up to 4 s in the sensor thread |
| S9 | `sensors_python.py:493-495` | Network rate divided by the nominal INTERVAL, not the elapsed time; first sample 0; counter resets give negative rates; errors print `-1.0 B/s` |
| S10 | `stats.py:649-683` | Only `/` (or the current Windows drive); disk in decimal GB while RAM is binary MiB, both labelled `M`/`G` |
| S11 | scheduler | Any uncaught exception ends that metric's thread for the session |
| S12 | `stats.py:887` | Weather request without timeout; rate-limit errors replace the description text |
| S13 | `sensors_librehardwaremonitor.py:317` | `used / total` raises ZeroDivisionError when LHM reports total 0; missing hardware nodes dereference `None` |
| S14 | `stats.py:852, 890` | Kelvin printed as `°K` |
| S15 | `sensors_python.py:151` | CPU temperature = first entry of the chip (not the max of cores, no Tdie/Tccd choice); `zenpower` only if `k10temp` is absent; no `acpitz`/`thinkpad`/`soc_thermal`/Super-I/O fallback; NaN on Windows/macOS with the Python backend |
| S16 | `sensors_python.py:249` | GPUtil spawns `nvidia-smi` on every call |
| S17 | `sensors_librehardwaremonitor.py:438, 448, 495, 498` | **inferred**: LHM `Data` sensors are GiB (2^30) but are multiplied by 1e9 and later divided by 2^20: RAM and network totals over-report by about 7.4 % |
| S18 | `stats.py:328-336` and siblings | An unsupported metric hides its widgets forever after one warning; the user sees the static background |
| S19 | `sensors_python.py` (fans) | The more accurate `pwmN` duty (0..255) is ignored in favour of an RPM ratio |

Vendor app (static):

| # | Issue |
|---|---|
| T1 | Sensor choices stored as localised label strings (and the network adapter by its localised name): they silently stop matching after a language or driver change |
| T2 | Only nine hard-wired pickers; everything else relies on fixed label lists, which differ between CPU/GPU vendors and engine versions |
| T3 | CPU temperature defaults to `CPU Package` and silently falls back to the maximum core temperature |
| T4 | Network speed shown as integer KB/s; values below 0.1 are treated specially (reported as "floors to 0.1"; exact rule unconfirmed) |
| T5 | `RAM_GB` = MB / 1024 labelled `G` (binary value, decimal-looking label) |
| T6 | FPS needs a third-party overlay tool running; otherwise the widget shows nothing useful |
| T7 | Requires administrator rights for all sensors |

## 6. Bezel strategy

### 6.1 Linux

| Metric | Source |
|---|---|
| CPU usage (total and per core) | `/proc/stat` deltas over measured elapsed time |
| CPU frequency | mean of `/sys/devices/system/cpu/cpu*/cpufreq/scaling_cur_freq` over online cores (fallback `cpuinfo_cur_freq`); per-core available |
| CPU temperature | hwmon by chip `name` and label: `coretemp` `Package id 0`; `k10temp` `Tctl`/`Tdie`/`Tccd*`; `zenpower`; `cpu_thermal`/`soc_thermal`; `acpitz` last. Document the `Tctl` offset. User can pick any hwmon input. |
| CPU power | RAPL energy counters under `/sys/class/powercap/intel-rapl*/energy_uj` (delta / elapsed). Reading them needs root or a udev/group rule on current kernels; AMD coverage depends on the kernel (verify). Unavailable otherwise. |
| CPU voltage | Super-I/O hwmon `in*_input` (`nct6775`, `it87`) when present; optional |
| Fans, pump | hwmon `fanN_input` (RPM) and `pwmN` (duty / 255 -> %); chosen by hwmon path and label; RPM and % kept separate |
| NVIDIA GPU | NVML: utilisation, memory used/total, temperature, fan %, graphics clock, power (no voltage) |
| AMD GPU | amdgpu sysfs: `gpu_busy_percent`, `mem_info_vram_used`/`mem_info_vram_total`, hwmon `temp1_input`, `power1_average` (or `power1_input`), `fan1_input`, `pwm1`, `in0_input` (vddgfx, mV), `pp_dpm_sclk` |
| Intel GPU | i915/xe sysfs frequency (`gt_cur_freq_mhz`); usage needs perf counters (optional) |
| RAM, swap | `/proc/meminfo`: `MemTotal - MemAvailable`, `SwapTotal - SwapFree` |
| Disk space | `statvfs` per chosen mount point |
| Disk I/O, temperature | `/proc/diskstats` (sectors x 512, elapsed-based); `drivetemp` or NVMe hwmon |
| Network | `/sys/class/net/<if>/statistics/{rx,tx}_bytes` with monotonic elapsed time, 64-bit wrap handling, "auto" = default-route interface, and sum of all |
| Volume | PipeWire / PulseAudio default sink |
| FPS | optional: MangoHud or gamescope statistics when available; unavailable otherwise |
| Weather | Open-Meteo (no key) with its geocoding API; cached, with a stale marker and timeouts |
| Ping | unprivileged ICMP (`SOCK_DGRAM`, `ping_group_range`) or TCP-connect fallback, in its own task; timeout = unavailable |
| Date/time | CLDR-compatible formatter (Python themes use CLDR patterns such as `yyyy.MM.dd`, `HH:mm:ss zzz`, `EEE d MMM`); the vendor SubName vocabulary as aliases |

### 6.2 Windows

| Metric | Source |
|---|---|
| CPU usage, RAM, disk space, network counters | Win32/PDH (`GetSystemTimes`, per-core PDH counters, `GlobalMemoryStatusEx`, `GetDiskFreeSpaceEx`, interface byte counters) with measured elapsed time |
| Temperatures, clocks, voltages, fans, power | LibreHardwareMonitor's WMI provider when LHM is running (namespace to be confirmed during implementation), or HWiNFO's shared-memory interface when the user enables it in HWiNFO. Bezel reads what is published; it does not bundle either engine or require kernel drivers of its own. |
| NVIDIA GPU | NVML |
| AMD / Intel GPU | vendor libraries where available; PDH "GPU Engine" counters for usage |
| FPS | RTSS shared memory when RivaTuner Statistics Server runs; unavailable otherwise |
| Volume | Core Audio default render endpoint |

### 6.3 Cadence

A single sampler fills a shared snapshot (for example 4 Hz for fast metrics, 1 Hz for disks, 10 min or more for
weather); renderers read the snapshot per frame. Slow sources (ping, weather, NVML initialisation) never block the
render path. Each sample carries its timestamp so history graphs use real time spacing.

## 7. Open questions

1. Units of LHM `Data` sensors in the version the Python project bundles (GiB versus GB, S17).
2. Exact LHM sensor names on AMD Ryzen and for the FPS factor; psutil's Windows load-average emulation cadence.
3. Whether ping3 works on Linux without `ping_group_range` or root.
4. The vendor engine's `code.ini` flag semantics (64 / 0 / 8192) and the drop-down population rules for fans and
   voltages.
5. NVIDIA memory units reported by GPUtil (assumed MiB).
