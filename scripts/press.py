#!/usr/bin/env python3
"""Presses device buttons over SSH, by writing input events to the gamepad's evdev node.

usage: scripts/press.py a wait2 menu down a   (waitN pauses N seconds)
"""
import struct, subprocess, sys, time
CODES = {"b": 0x130, "a": 0x131, "x": 0x133, "y": 0x134, "select": 0x13A, "start": 0x13B,
         "menu": 0x13C, "l1": 0x136, "r1": 0x137, "l2": 0x138, "r2": 0x139,
         "up": 0x220, "down": 0x221, "left": 0x222, "right": 0x223,
         "voldown": 114, "volup": 115}
SSH = ["ssh", "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null",
       "-o", "LogLevel=ERROR", "root@10.35.35.1"]
def event(kind, code, value):
    return struct.pack("<qqHHi", 0, 0, kind, code, value)
for name in sys.argv[1:]:
    if name.startswith("wait"):
        time.sleep(float(name[4:])); continue
    code = CODES[name]
    # By name, as the event numbers change when a device is added, as the motor was.
    device_name = "gpio-keys-volume" if name.startswith("vol") else "gpio-keys-gamepad"
    data = event(1, code, 1) + event(0, 0, 0)
    up = event(1, code, 0) + event(0, 0, 0)
    # Press and release in one connection, with a short hold between, so a press that
    # restarts the SSH server (Settings > ADVANCED > SSH) still gets its release.
    find = ("for d in /sys/class/input/event*; do "
            f"[ \"$(cat $d/device/name)\" = {device_name} ] && dev=/dev/input/${{d##*/}}; done; ")
    script = find + f"dd bs={len(data)} count=1 of=$dev 2>/dev/null; sleep 0.1; cat > $dev"
    subprocess.run(SSH + [script], input=data + up, check=True)
    time.sleep(0.3)
