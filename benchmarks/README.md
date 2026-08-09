# Benchmarks

Honest, reproducible speed numbers.

PlainText is a **tree-walking interpreter** built for readability, not raw throughput. On tight,
compute-bound loops it is **slower** than an optimized bytecode VM like CPython — and that's the
expected trade-off, not a bug.

## `loop` — a compute-bound loop (5,000,000 iterations)

The same algorithm in each language ([`loop.pt`](loop.pt) / [`loop.py`](loop.py)), each timing
only its own loop with its own clock. Release build, Windows desktop:

| Language | Time (lower is better) |
|---|---|
| Python 3.14 | ~0.47 s |
| PlainText 2.2.0 | ~1.47 s |

So PlainText is roughly **3× slower** than Python on pure arithmetic loops. That's fine for what
PlainText is *for* — games, desktop UIs, and learning, where the per-frame logic is small and the
heavy lifting (drawing, audio) happens in native Raylib. It also means PlainText is **not** the
right tool for heavy number-crunching. Where that matters most — neural networks — the `ai` module
sidesteps the interpreter entirely, running training in compiled Rust with an optional GPU backend.

## Reproduce

```
cargo build --release
./target/release/plaintext run benchmarks/loop.pt
python benchmarks/loop.py
```

Numbers vary by machine; what matters is the ratio on the *same* machine.
