"""Runs tools/perf/board_bench.txt over board_gen.py perf scenes and prints the bench lines.

usage: python3 tools/perf/board_bench.py [n ...]            default 200 1000 3000 10000
  FDL_BOARD_RENDER=gpui|vello|auto picks the board backend (auto: by visible elements, see src/board/render.rs).
Needs `cargo build --release --features devtools`. Scenes and runs go to $TMPDIR/board-bench.
"""
import os, subprocess, sys, tempfile
here = os.path.dirname(os.path.abspath(__file__)); repo = os.path.dirname(os.path.dirname(here))
root = os.path.join(tempfile.gettempdir(), 'board-bench')
for n in [int(a) for a in sys.argv[1:]] or [200, 1000, 3000, 10000]:
    lib = os.path.join(root, f'lib-{n}')
    if not os.path.exists(os.path.join(lib, f'bench-{n}.excalidraw')):
        subprocess.run([sys.executable, os.path.join(here, 'board_gen.py'), 'perf', str(n), lib], check=True, stdout=subprocess.DEVNULL)
    work = os.path.join(root, f'run-{n}-{os.environ.get("FDL_BOARD_RENDER", "auto")}')
    subprocess.run([sys.executable, os.path.join(repo, 'tools/harness/run.py'), os.path.join(here, 'board_bench.txt'), lib, work], check=True)
    print(f'== {n} shapes ({os.environ.get("FDL_BOARD_RENDER", "auto")})')
    for l in open(os.path.join(work, 'log'), errors='replace'):
        if l.startswith('bench'): print(l.rstrip())
    sys.stdout.flush()
