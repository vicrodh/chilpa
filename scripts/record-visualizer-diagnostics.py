#!/usr/bin/env python3
"""Open the normal player with an opt-in visualizer probe and retain only its numeric report."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("binary",type=Path)
parser.add_argument("--output",type=Path,required=True)
args=parser.parse_args()
fixture=Path(__file__).resolve().parent/"fixtures/visualizer-diagnostics.qml"
environment=dict(os.environ,CHILPA_QML_SMOKE=str(fixture))
args.output.parent.mkdir(parents=True,exist_ok=True)
with args.output.open("w") as log:
    process=subprocess.Popen([str(args.binary.resolve())],env=environment,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,errors="replace")
    records=0
    print("Close other Chilpa instances before this run. Play the usual source, check Queues, then open the immersive view. Close Chilpa when finished.",flush=True)
    try:
        for line in process.stdout:
            marker="CHILPA_VIZ_DIAG "
            if marker not in line:continue
            try:record=json.loads(line.split(marker,1)[1])
            except json.JSONDecodeError:continue
            log.write(json.dumps(record)+"\n");log.flush()
            if "sample" in record:records+=1;print("Recorded sample "+str(record["sample"]),flush=True)
    except KeyboardInterrupt:
        process.terminate()
    finally:
        process.wait()
print("Report: "+str(args.output.resolve()))
if not records:
    print("No diagnostic samples were recorded. Close other Chilpa instances and run again.",file=sys.stderr)
    sys.exit(1)
sys.exit(process.returncode)
