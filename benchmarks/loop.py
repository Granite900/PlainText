# The same compute-bound loop as loop.pt, timed with Python's own clock.
# Run with:  python benchmarks/loop.py

import time

n = 5000000
total = 0.0
i = 0

start = time.perf_counter()
while i < n:
    total = total + (i % 7) * 0.5 - (i % 3)
    i = i + 1
elapsed = time.perf_counter() - start

print(f"python: {round(elapsed, 3)} s   (checksum {total})")
