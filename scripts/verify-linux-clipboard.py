#!/usr/bin/env python3
"""X11 system clipboard exercise. Run inside a dedicated Xvfb, not a user's display."""
import os
from pathlib import Path
import subprocess
import time

root = Path(__file__).resolve().parent.parent
target = (root / os.environ.get("CARGO_TARGET_DIR", "target")).resolve()
fixture = '{\r\n\t"中文😀": "<b>literal</b>",\n\t"ok": true\r\n}\r'.encode("utf-8")
owner = subprocess.Popen(["xclip", "-selection", "clipboard", "-in", "-quiet"], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    owner.stdin.write(fixture)
    owner.stdin.close()
    for _ in range(60):
        result = subprocess.run(["xclip", "-selection", "clipboard", "-out"], capture_output=True)
        if result.returncode == 0 and result.stdout == fixture:
            break
        time.sleep(0.05)
    else:
        raise RuntimeError("X11 clipboard fixture did not become available")
    subprocess.run([str(target / "debug/examples/clipboard_smoke"), str(target / "linux-clipboard.png")], check=True, timeout=30)
    print("LINUX_X11_CLIPBOARD_PASS: actual X selection, Unicode/CRLF copy/cut/paste/undo")
finally:
    if owner.poll() is None:
        owner.terminate()
    owner.wait(timeout=5)
