# 13. Neural networks

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

## Save a trained brain

Training takes time, so save the result and load it back later — even in another program:

```plaintext
brain.save("brain.ai")
smart = load_network("brain.ai")
print(smart.predict([1, 0]))
```

[`learn.pt`](../../examples/learn.pt) trains XOR (with `device: auto`), then saves and reloads.

## Load data from a file

Typing examples out by hand gets old fast. Put them in a `.csv` file instead — one row per
example, input columns first, answer column(s) last — and load it:

```plaintext
data = load_dataset("training.csv", outputs: 1)
examples = data[0]
answers  = data[1]
brain.train(examples, answers, epochs: 500, optimizer: adam)
```

A header row of column names is skipped for you. `read_csv("file.csv")` is the simpler tool when
you just want the raw rows of numbers. [`dataset.pt`](../../examples/dataset.pt) trains on a real
CSV of labelled points, then **scores accuracy on fresh random points** it never trained on.

## Neuroevolution — learning with no answers

Backprop needs the right answer for every example. But how do you train a character to play a
game, where the only feedback is "you got a score of 240"? You **evolve** it:

1. Make a whole **population** of random networks.
2. Let each one try, and give it a **score** (fitness) for how well it did.
3. Keep the best, breed the winners into a new generation with small random changes, and repeat.

```plaintext
brains = population(count: 100, inputs: 4, hidden: [8], outputs: 2)

// each generation:
scores = [ ... ]                              // one number per brain — higher is better
champion = best_of(brains, scores)            // the current star (draw it, or save it)
brains = evolve(brains, scores, mutation: 0.1, keep: 4)
```

`brains` is just a list — use `brains[i].predict(...)` to drive agent `i`. No training data at
all; the population teaches itself. [`evolve.pt`](../../examples/evolve.pt) is a whole crowd of
dots that learn to steer to a goal this way — watch the generation counter climb.

```bash
plaintext run examples/evolve.pt
```

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
tiny one like XOR the CPU is often faster. [`learn.pt`](../../examples/learn.pt) and
[`dataset.pt`](../../examples/dataset.pt) both use `device: auto`.

You can also drive training from a game loop with `brain.train_once(...)` if you want to draw
progress yourself — see the language reference.

## Practice

| Example | Idea |
|---------|------|
| [`learn.pt`](../../examples/learn.pt) | XOR + GPU auto + save / load |
| [`dataset.pt`](../../examples/dataset.pt) | CSV training + accuracy score |
| [`evolve.pt`](../../examples/evolve.pt) | Neuroevolution in a window |

Full index: [`examples/README.md`](../../examples/README.md).

## Common mistakes

| Mistake | Fix |
|---------|-----|
| `neural_network` "needs the ai module" | Add `import ai` at the top |
| Examples and answers don't line up | Same count; match `inputs` / `outputs` widths |
| Outputs never get close | Scale your answers to the 0–1 range |
| Expecting images or text | This is a small numeric network — keep inputs numeric and small |

## That's the tour

You've seen the main toolkit. Keep building — and when something breaks, check
[troubleshooting](../troubleshooting.md).

**Previous:** [Game kit ←](12-game-kit.md) · **Next:** [Talking to the web →](14-web.md)
