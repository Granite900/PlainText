# 12. Neural networks

**Goal:** Teach a small network a pattern from examples, then ask it to predict — and,
if you have a graphics card, train on it.

This is a bonus lesson. Nothing else in the language depends on it, but it reuses
everything you already know: lists, loops, and functions.

## The idea

A neural network learns a rule from **example → answer** pairs instead of you writing the
rule by hand. You show it inputs and the answers you want; it adjusts itself until its own
guesses match; then it can answer inputs it has never seen.

Turn it on with `import ai`.

```plaintext
import ai

// 2 inputs → one hidden layer of 8 → 1 output
brain = neural_network(inputs: 2, hidden: [8], outputs: 1)

examples = [[0, 0], [0, 1], [1, 0], [1, 1]]
answers  = [[0],    [1],    [1],    [0]]        // XOR: true when exactly one input is 1

brain.train(examples, answers, epochs: 3000, optimizer: adam, rate: 0.05)

print(brain.predict([1, 0]))   // close to [1]
print(brain.predict([1, 1]))   // close to [0]
```

`examples` and `answers` are lists of lists of numbers, and they must line up: the same
count, each example as wide as `inputs`, each answer as wide as `outputs`.

## Building one

`neural_network(inputs:, hidden:, outputs:)`. `hidden` is either one number (a single
hidden layer) or a list of numbers — one per layer — so you pick both how many hidden
layers and how big each one is:

```plaintext
big = neural_network(inputs: 4, hidden: [16, 12, 8], outputs: 3)
```

## Training settings

`brain.train(examples, answers, ...settings)` — every setting is optional:

| Setting | What it does | Default |
|---------|--------------|---------|
| `epochs` | how many passes over the data | 1000 |
| `optimizer` | the update rule: `sgd`, `momentum`, `rmsprop`, `adam` | `sgd` |
| `rate` | learning rate — how big each step is | per optimizer |
| `decay` | slowly shrinks the rate as training goes on | 0 (off) |

`brain.predict(row)` returns a list — one number per output — so read a single output with
`brain.predict(row)[0]`.

## A real task

XOR is a toy. [`classify.pt`](../../examples/classify.pt) is closer to real machine
learning: it makes a training set from random points, learns "is this point inside a
circle?", then **scores its accuracy on fresh points it never trained on**.

```bash
plaintext run examples/classify.pt
```

## Save a trained brain

Training takes time, so save the result and load it back later — even in another program:

```plaintext
brain.save("brain.ai")
smart = load_network("brain.ai")
print(smart.predict([1, 0]))
```

See [`remember.pt`](../../examples/remember.pt) for the full round-trip.

## Train on a GPU

Add `device:` when you build the network to train on a graphics card. One backend covers
every vendor:

```plaintext
brain = neural_network(inputs: 2, hidden: [16, 12], outputs: 1, device: auto)
```

| `device:` | Where it trains |
|-----------|-----------------|
| `auto` (or `gpu`) | any GPU the machine has |
| `cuda` | an NVIDIA GPU |
| `rocm` | an AMD GPU |
| `mps` | an Apple GPU (Metal) |
| `vulkan` / `dx12` | a specific backend |
| `cpu` | force the CPU |

If the GPU can't be opened (say you ask for `cuda` with no NVIDIA card), training **falls
back to the CPU and says so** rather than failing. GPUs pay off for large networks — for a
tiny one like XOR the CPU is actually faster, since there's barely any work to speed up.

```bash
plaintext run examples/gpu_learn.pt
```

## Watch it learn

Because you can run a single round at a time with `brain.train_once(...)`, training fits
inside a game loop and can draw its own progress:

```bash
plaintext run examples/watch_learn.pt
```

## Practice

| Example | Idea |
|---------|------|
| [`learn.pt`](../../examples/learn.pt) | Train XOR from scratch |
| [`classify.pt`](../../examples/classify.pt) | A real task + an accuracy score |
| [`remember.pt`](../../examples/remember.pt) | `save` and `load_network` |
| [`gpu_learn.pt`](../../examples/gpu_learn.pt) | Train on a GPU |
| [`watch_learn.pt`](../../examples/watch_learn.pt) | Watch it learn live |

## Common mistakes

| Mistake | Fix |
|---------|-----|
| `neural_network` "needs the ai module" | Add `import ai` at the top |
| Examples and answers don't line up | Same count; match `inputs` / `outputs` widths |
| Outputs never get close | Scale your answers to the 0–1 range |
| Expecting images or text | This is a small numeric network — keep inputs numeric and small |

## That's the tour

Where to go from here:

1. Keep the [cheatsheet](../cheatsheet.md) open while you write.
2. Reach for the [language reference](../language-reference.md) when you need an API detail.
3. Copy freely from [`examples/`](../../examples/).
4. If something breaks, check [troubleshooting](../troubleshooting.md).

**Previous:** [Desktop UI ←](11-ui.md) · [Docs home ↑](../README.md)
