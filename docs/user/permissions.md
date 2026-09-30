# Let Bezel open the screen

[Português (Brasil)](pt-BR/permissions.md)

Bezel never asks for administrator rights and never installs drivers. What the
system needs, once, depends on how you installed it.

## Linux

**deb and rpm packages:** nothing to do. The package installs a udev rule that
gives the person signed in at the computer access to every supported screen.
If the screen was plugged in before the installation, unplug it and plug it back.

**AppImage, the command-line archive, or a build from source:** install the rule
once. `bezel udev-rules` prints the rule and, below it, the one command that
installs it:

```bash
bezel udev-rules
```

Copy that command and run it in a terminal. It looks like this (it asks for your
password because it writes to `/etc`):

```bash
bezel udev-rules 2>/dev/null | sudo tee /etc/udev/rules.d/60-bezel.rules >/dev/null && sudo udevadm control --reload && sudo udevadm trigger
```

The app does the same: when Linux refuses the screen's port, Bezel shows the
command, ready to copy. Bezel never runs it for you.

Then unplug and replug the screen, and check that it shows up:

```bash
bezel devices
```

The rule covers serial screens (`/dev/ttyACM*`), the USB bulk families (Turing
USB, WCH) and Turing USB panels in desktop mode (`hidraw`). It only grants access
to the user signed in at the computer; it does not change anything else.

## Windows

- **Serial screens** (Turing rev A and rev C, XuanFang, Kipye, WeAct): Windows'
  own driver (usbser) handles them; they appear as COM ports. Nothing to install.
- **Turing USB generation (USB id `1CBE:xxxx`) and WCH panels (`43A8:xxxx`):**
  they need the WinUSB driver. If `bezel devices` does not list the screen and
  Device Manager shows it without a driver:
  1. Download Zadig from [zadig.akeo.ie](https://zadig.akeo.ie).
  2. In Zadig, choose **Options → List All Devices**, then pick the screen in the
     list (check that the USB ID starts with `1CBE` or `43A8`).
  3. Select **WinUSB** as the driver and click **Install Driver** (or
     **Replace Driver**).

  To undo it, open Device Manager, right-click the screen, choose **Uninstall
  device** and tick the option that deletes the driver.
- **Turing USB panels in desktop mode** use Windows' HID driver: nothing to
  install. See [Supported screens](devices.md#desktop-mode).
- **Sensors:** CPU temperature, fans and power come from LibreHardwareMonitor,
  which must be running as administrator. See [Sensors](sensors.md#windows).
