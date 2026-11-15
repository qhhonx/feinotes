"""Memory at each `shot` of a harness script (macOS, `footprint`).

usage: python3 tools/perf/mem_probe.py <script.txt> <lib_dir> <work_dir> [label]
Runs tools/harness/run.py (FDL_BIN picks the binary) and, whenever the script
saves a shot, measures the process with `footprint`. Put a `wait` after each
`shot` so the app sits still while it is measured. Prints one row per shot:
total footprint and the big categories, in MB:
  pixels  MALLOC_LARGE + untagged VM (decoded images and other big buffers;
          with src/big_alloc.rs those are mmapped, so untagged)
  small   MALLOC_SMALL + MALLOC_TINY
  gpu     IOAccelerator + owned unmapped (graphics): textures, buffers
  surface IOSurface: the window's drawables
  cg      CG image
Appends the rows as JSON lines to <work_dir>/mem.jsonl.
"""
import json, os, re, subprocess, sys, time

here = os.path.dirname(os.path.abspath(__file__))
script, lib, work = (os.path.abspath(a) for a in sys.argv[1:4])
label = sys.argv[4] if len(sys.argv) > 4 else os.path.basename(work)
os.makedirs(work, exist_ok=True)
log = os.path.join(work, 'log')
if os.path.exists(log): os.remove(log)
run = subprocess.Popen([sys.executable, os.path.join(here, '..', 'harness', 'run.py'), script, lib, work])
binary = os.path.realpath(os.environ.get('FDL_BIN') or os.path.join(here, '..', '..', 'target', 'release', 'quiet_md'))

def pid():
    out = subprocess.run(['pgrep', '-f', '^' + re.escape(binary)], capture_output=True, text=True).stdout.split()
    return out[0] if out else None

def mb(v, unit):
    return float(v) * {'B': 1 / 1048576, 'KB': 1 / 1024, 'MB': 1, 'GB': 1024}[unit]

def measure(p):
    text = subprocess.run(['footprint', '-p', p], capture_output=True, text=True).stdout
    total = re.search(r'Footprint: ([\d.]+) (\w+)', text)
    cats = {}
    for m in re.finditer(r'^\s*([\d.]+) (\w+)\s+[\d.]+ \w+\s+[\d.]+ \w+\s+\d+\s+(.+)$', text, re.M):
        name = m.group(3).strip().lower().replace('_', ' ')
        cats[name] = cats.get(name, 0) + mb(m.group(1), m.group(2))
    # Category names vary by macOS version (MALLOC_LARGE / Malloc Large).
    g = lambda *ks: round(sum(cats.get(k, 0) for k in ks), 1)
    return {'total': round(mb(*total.groups()), 1) if total else None,
            'pixels': g('malloc large', 'untagged (vm allocate)', 'untagged'),
            'small': g('malloc small', 'malloc tiny'), 'gpu': g('ioaccelerator (graphics)', 'ioaccelerator', 'owned physical footprint (unmapped) (graphics)'),
            'surface': g('iosurface'), 'cg': g('cg image')}

seen, rows = set(), []
while run.poll() is None:
    time.sleep(0.2)
    if not os.path.exists(log): continue
    for name in re.findall(r'devtools: saved .*/([^/\s]+)\.png', open(log, errors='replace').read()):
        if name in seen: continue
        seen.add(name)
        p = pid()
        if not p: continue
        time.sleep(0.5)
        row = {'label': label, 'shot': name, **measure(p)}
        rows.append(row)
        print(f"{label:>12} {name:>10}  total {row['total']:6.0f}  pixels {row['pixels']:5.0f}  small {row['small']:4.0f}  "
              f"gpu {row['gpu']:5.0f}  surface {row['surface']:3.0f}  cg {row['cg']:3.0f}", flush=True)
with open(os.path.join(work, 'mem.jsonl'), 'a') as f:
    for r in rows: f.write(json.dumps(r) + '\n')
