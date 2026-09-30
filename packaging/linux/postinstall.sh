#!/bin/sh
# Post-install script of the Bezel deb and rpm packages. Applies the packaged
# udev rule without a reboot so the logged-in user can open the screen now.
# Best effort: a container or a chroot has no udev, and nothing here may fail
# the installation.
udevadm control --reload 2>/dev/null || true
udevadm trigger --subsystem-match=tty 2>/dev/null || true
udevadm trigger --subsystem-match=usb 2>/dev/null || true
exit 0
