"""Run the public-link UI flow in a fresh offline Mac library."""

import os
from pathlib import Path
import subprocess
import tempfile
import uuid

STEPS = '''wait 1500
resize 1050 760
sharing-ui public-link-prepare
wait 1200
sharing-ui public-link-retry-seed
wait 1200
sharing-ui public-link-rejected
wait 600
sharing-ui public-link-opened
wait 600
sharing-ui public-link-retry-conflict
wait 1200
sharing-ui public-link-retry-reply
wait 1200
sharing-ui public-link-published
wait 600
sharing-ui public-link-recopy
wait 600
sharing-ui public-link-close
wait 600
sharing-ui public-link-closed
wait 600
sharing-ui public-link-copy-denied
sharing-ui public-link-explicit-invite
sharing-ui public-link-new-editor
wait 1200
sharing-ui public-link-new-editor-published
sharing-ui public-link-stop-recover
quit
'''

repo = Path(__file__).resolve().parents[2]
binary = Path(os.environ.get("FDL_CLOUD_BIN", repo / "target/debug/quiet_md"))
bridge = os.environ.get("FDL_CLOUD_DYLD_PATH")
if not bridge:
    candidates = (repo / "target/debug/build").glob("quiet_md-*/out/libQuietCloudRestore.dylib")
    bridge = str(max(candidates, key=lambda path: path.stat().st_mtime).parent)
root = Path(tempfile.mkdtemp(prefix="quiet-sync-qa-public-link-", dir="/private/tmp"))
script = root / "script.txt"
script.write_text(STEPS)
log = root / "run.log"
env = dict(os.environ, TMPDIR="/private/tmp", QUIET_SYNC_QA_ROOT=str(root), QUIET_SYNC_QA_CHANNEL=str(uuid.uuid4()),
           QUIET_SYNC_QA_SHARED_SESSIONS="1", QUIET_SYNC_QA_SHARING_UI="1", FDL_SCRIPT=str(script),
           FDL_NOBLINK="1", QUIET_MD_LOG_DIR=str(root / "logs"), DYLD_LIBRARY_PATH=bridge)
print(f"Artifacts: {root}", flush=True)
with log.open("w") as output:
    subprocess.run([str(binary)], cwd=root, env=env, stdout=output, stderr=subprocess.STDOUT, timeout=60, check=True)
lines = log.read_text().splitlines()
for marker in ("PUBLIC_LINK_UI_OK", "PUBLIC_LINK_EDIT_UI_OK", "PUBLIC_LINK_STOP_RECOVERY_OK"):
    assert any(marker in line for line in lines), "\n".join(lines[-25:])
    print(next(line for line in lines if marker in line))
