#!/usr/bin/env python3
"""Requires a graphical desktop. Uses scratch workspace files, never the user's data."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--screenshots", type=Path, default=Path("target/native-persistence"))
    options = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    cargo = ["cargo"] if shutil.which("cargo") else [str(root / "scripts/cargo.sh")]
    subprocess.run(cargo + ["build", "--locked", "--example", "persistence_smoke"], cwd=root, check=True)
    target_dir = (root / os.environ.get("CARGO_TARGET_DIR", "target")).resolve()
    binary = target_dir / "debug/examples" / ("persistence_smoke.exe" if os.name == "nt" else "persistence_smoke")
    output = (root / options.screenshots).resolve()
    output.mkdir(parents=True, exist_ok=True)

    def run(mode, directory, expected=0):
        result = subprocess.run([str(binary), mode, str(directory)], capture_output=True, text=True, timeout=30)
        print(result.stdout, end="", flush=True)
        if result.stderr:
            print(result.stderr, end="", flush=True)
        if result.returncode != expected:
            raise RuntimeError(f"{mode}: expected exit {expected}, got {result.returncode}")

    with tempfile.TemporaryDirectory(prefix="tabloom-native-persistence-") as scratch:
        scratch = Path(scratch)
        normal = scratch / "normal"
        run("write", normal)
        run("read", normal)
        shutil.copy2(normal / "restored.png", output / "step3-restored.png")
        # Only deliberately corrupt the disposable fixture created above.
        (normal / "session.json").write_bytes(b"corrupt main for native recovery verification")
        run("read", normal)
        shutil.copy2(normal / "restored.png", output / "step3-backup-recovery.png")
        abnormal = scratch / "abnormal"
        run("crash", abnormal, expected=23)
        assert (abnormal / "session.json").exists(), "must leave a committed manifest"
        run("read", abnormal)
        shutil.copy2(abnormal / "restored.png", output / "step3-crash-recovery.png")
    print("NATIVE_PERSISTENCE_SCENARIOS_PASS", flush=True)


if __name__ == "__main__":
    main()
