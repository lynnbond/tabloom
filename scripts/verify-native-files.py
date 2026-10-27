#!/usr/bin/env python3
"""Native renderer + file/recovery integration. Requires a graphical desktop."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
cargo = ["cargo"] if shutil.which("cargo") else [str(root / "scripts/cargo.sh")]
subprocess.run(cargo + ["build", "--example", "file_smoke", "--locked"], cwd=root, check=True)
target_dir = (root / os.environ.get("CARGO_TARGET_DIR", "target")).resolve()
binary = target_dir / "debug/examples" / ("file_smoke.exe" if os.name == "nt" else "file_smoke")
with tempfile.TemporaryDirectory(prefix="tabloom-files-") as scratch:
    scratch = Path(scratch)
    original = '{\r\n\t"中文😀": true\n}\r'
    (scratch / "source.json").write_bytes(b"\xff\xfe" + original.encode("utf-16-le"))
    subprocess.run([str(binary), str(scratch), "write"], check=True, timeout=30)
    # Only disposable fixtures are modified; formal user data is never opened.
    (scratch / "source.json").write_text("external change", encoding="utf-8")
    subprocess.run([str(binary), str(scratch), "read"], check=True, timeout=30)
    output = target_dir / "native-files"
    output.mkdir(parents=True, exist_ok=True)
    for name in ["files.png", "restored.png"]:
        shutil.copy2(scratch / name, output / name)
print("NATIVE_FILE_SCENARIOS_PASS")
