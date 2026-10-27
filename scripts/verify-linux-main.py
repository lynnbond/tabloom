#!/usr/bin/env python3
"""Launch the real Linux application binary with an isolated standard data directory."""
import os
from pathlib import Path
import json
import subprocess
import tempfile
import time

root = Path(__file__).resolve().parent.parent
target = (root / os.environ.get("CARGO_TARGET_DIR", "target")).resolve()
with tempfile.TemporaryDirectory(prefix="tabloom-main-") as scratch:
    for selected in (False,True):
        case = Path(scratch) / ("chosen" if selected else "default")
        env = dict(os.environ, XDG_DATA_HOME=str(case / "data"), XDG_CONFIG_HOME=str(case / "config"))
        session = case / "data/tabloom/session.json"
        if selected:
            directory = case / "自选工作区 folder"
            directory.mkdir(parents=True)
            choice = case / "config/tabloom/workspace-location.json"
            choice.parent.mkdir(parents=True)
            choice.write_text(json.dumps({"version":1,"directory":str(directory)}),encoding="utf-8")
            session = directory / "session.json"
        process = subprocess.Popen([str(target / "debug/tabloom")], env=env)
        try:
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"application exited early: {process.returncode}")
                if session.exists():
                    manifest = json.loads(session.read_text(encoding="utf-8"))
                    assert manifest["version"] == 1
                    assert len(manifest["workspaces"]) == 1
                    data = json.loads((session.parent / "workspaces" / manifest["workspaces"][0]).read_text(encoding="utf-8"))
                    assert len(data["regions"]) == 1
                    break
                time.sleep(0.05)
            else:
                raise RuntimeError("application did not initialize selected data path")
            time.sleep(0.5)
            assert process.poll() is None
            print(f"LINUX_MAIN_STARTUP_PASS: {'selected directory' if selected else 'default directory'} + real executable + JSON workspace")
        finally:
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=10)
