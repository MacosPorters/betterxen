# SPDX-License-Identifier: GPL-3.0-or-later
"""One Instruments capture attached to the verified synthetic guest."""
import importlib.util
import json
import subprocess
import sys
import time
from pathlib import Path

selection = sys.argv.pop(1)
if selection in {'wb', 'wc', 'upload-wb', 'default'}:
    workload = ('coherence', [selection])
elif selection.startswith('copy-') and selection[5:] in {'wb', 'wc', 'upload'}:
    workload = ('copy', ['trace-copy', selection[5:]])
else:
    sys.exit('Select wb, wc, upload-wb, default, copy-wb, copy-wc or copy-upload.')
runner_path = Path(__file__).resolve().with_name('run.py')
spec = importlib.util.spec_from_file_location("b_runner", runner_path)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
detect = runner.backend
trace = None
args = None
log_file = None

def attach(exe, env, cwd):
    global trace, args, log_file
    result = detect(exe, env, cwd)
    if not result["verified"]:
        return result
    # Attach only to the verified private guest, before releasing its handshake.
    args = ["xcrun", "xctrace", "record", "--template", "Metal System Trace",
            "--attach", result["guest_pid"], "--time-limit", "5s",
            "--no-prompt", "--output", str(cwd / "gpu.trace")]
    log_file = cwd / "xctrace-output.txt"
    trace_env = env | {"DEVELOPER_DIR": "/Applications/Xcode.app/Contents/Developer"}
    with log_file.open("wb") as log:
        trace = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=log,
                                 stderr=subprocess.STDOUT, env=trace_env, cwd=cwd,
                                 start_new_session=True)
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline and trace.poll() is None:
        if b"Ctrl-C" in log_file.read_bytes():
            time.sleep(0.3)
            break
        time.sleep(0.1)
    return result

runner.backend = attach
runner.cases_for = lambda group, seed: [workload]
try:
    status = runner.main()
finally:
    if trace:
        try:
            result_code = trace.wait(12)
        except subprocess.TimeoutExpired:
            trace.terminate()
            result_code = trace.wait(5)
        receipt = {"command": args, "exit": result_code, "xcrun_developer_dir":
                   "/Applications/Xcode.app/Contents/Developer", "log": log_file.name,
                   "trace_exists": (log_file.parent / "gpu.trace").exists()}
        (log_file.parent / "xctrace.json").write_text(json.dumps(receipt, indent=2) + "\n")
sys.exit(status)
