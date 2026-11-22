# Editor performance benchmarks

The README reports results from this procedure. Use the same scripts to reproduce it on your own machine.

## Environment

| Item | Value |
|---|---|
| Date | 2026-11-03 |
| Source | `0e92b51`, built with `cargo build --release --locked` |
| Machine | Apple M5 Pro, 48 GiB, macOS 26.5 |
| Window | 1280 × 800 pt at 2× scale, Craft theme |
| Document | 1,001,307 characters in 549 chapters with paragraphs, lists, tables and code, created by the [sample generator](../tools/perf/gen_library.py) |

## Method

The [harness](../tools/harness/README.md) replays [readme-performance.txt](../tools/harness/examples/readme-performance.txt). It opens the document, types 100 individual `x` characters at the end of the first paragraph, presses Backspace 100 times, then scrolls down 100 times.

After each operation, `bench` synchronously calls `window.draw` and records the frame processing time as `frame`. P50 is the median and P95 the 95th percentile. These figures measure in-app processing on one machine. End-to-end keyboard-to-screen latency also includes presentation time. No other apps were benchmarked.

## Results

| Operation | Frame P50 | Frame P95 | Maximum |
|---|---:|---:|---:|
| Typing | 5.06 ms | 5.78 ms | 6.94 ms |
| Backspacing | 4.99 ms | 5.55 ms | 5.69 ms |
| Scrolling | 2.08 ms | 4.14 ms | 9.23 ms |

Raw output in milliseconds:

```text
bench "type x" x100: step [p50 0.03 p95 0.05 max 5.29] frame [p50 5.06 p95 5.78 max 6.94] editor render [p50 3.77 p95 4.31 max 4.99] ms
bench "key backspace" x100: step [p50 0.02 p95 0.05 max 3.91] frame [p50 4.99 p95 5.55 max 5.69] editor render [p50 3.64 p95 4.12 max 4.24] ms
bench "wheel 900 500 40" x100: step [p50 0.01 p95 0.02 max 0.02] frame [p50 2.08 p95 4.14 max 9.23] editor render [p50 0.02 p95 0.03 max 1.83] ms
```

## Reproduce the benchmark

The test uses a temporary library and leaves `~/Documents/Quiet.md` untouched.

```sh
cargo build --release --locked --features devtools
python3 tools/perf/gen_library.py /tmp/quiet-perf/library
printf '{"english":true,"theme":"craft"}\n' > /tmp/quiet-perf/library/preferences.json
python3 tools/harness/run.py tools/harness/examples/readme-performance.txt /tmp/quiet-perf/library /tmp/quiet-perf/run
grep -a bench /tmp/quiet-perf/run/log
```
