#!/usr/bin/env python3
"""Measure a disposable native memory_probe; never attaches to the user's app."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import sys
import time

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary",type=Path)
parser.add_argument("--syntax",default="plain")
args=parser.parse_args()
if sys.platform!="darwin":parser.error("requires macOS vmmap")
process=subprocess.Popen([str(args.binary.resolve()),args.syntax],stdout=subprocess.PIPE,text=True)
try:
    # The fixture exits itself after 12 seconds, so this read is bounded by its lifetime.
    while True:
        line=process.stdout.readline()
        if not line:raise RuntimeError("fixture exited before ready")
        if "MEMORY_READY" in line:break
    samples=[]
    for _ in range(3):
        result=subprocess.run(["vmmap","-summary",str(process.pid)],capture_output=True,text=True,check=True,timeout=10).stdout
        match=re.search(r"Physical footprint:\s+([\d.]+)([KMG])",result)
        if not match:raise RuntimeError("vmmap footprint unavailable")
        mb=float(match[1])*{"K":1/1024,"M":1,"G":1024}[match[2]]
        rss=int(subprocess.check_output(["ps","-p",str(process.pid),"-o","rss="],text=True).strip())/1024
        samples.append({"physical_footprint_mib":mb,"rss_mib":round(rss,2)})
        time.sleep(0.5)
    print(json.dumps({"binary":str(args.binary),"syntax":args.syntax,"window_points":[1100,760],"samples":samples},indent=2))
finally:
    if process.poll() is None:
        process.wait(timeout=20)
