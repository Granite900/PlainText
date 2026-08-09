//! A small feed-forward neural network (multilayer perceptron) with backprop.
//!
//! Deliberately from scratch and dependency-free: dense layers, a sigmoid
//! activation, mean-squared-error loss, and a choice of optimizers (plain SGD,
//! momentum, RMSProp, Adam). It works on `f64` — the language's one Number
//! type — so a PlainText program can create one, train it on lists of numbers,
//! and ask it to predict. Not built for big/deep models; built to be learnable.

use std::cell::Cell;

use crate::gpu::DeviceKind;

/// Which weight-update rule to use.
#[derive(Clone, Copy, PartialEq)]
pub enum Opt {
    Sgd,
    Momentum,
    RmsProp,
    Adam,
}

impl Opt {
    pub fn from_name(name: &str) -> Option<Opt> {
        match name.to_lowercase().as_str() {
            "sgd" => Some(Opt::Sgd),
            "momentum" => Some(Opt::Momentum),
            "rmsprop" | "rms" => Some(Opt::RmsProp),
            "adam" => Some(Opt::Adam),
            _ => None,
        }
    }

    /// Numeric id shared with the GPU shader's optimizer switch.
    pub fn id(self) -> u32 {
        match self {
            Opt::Sgd => 0,
            Opt::Momentum => 1,
            Opt::RmsProp => 2,
            Opt::Adam => 3,
        }
    }
}

/// One training run's settings.
pub struct TrainCfg {
    pub opt: Opt,
    pub rate: f64,
    pub decay: f64,
}

/// A fully-connected layer: `out * in` weights (row-major, `w[o*in + i]`), one
/// bias per output, plus per-weight optimizer state.
#[derive(Clone)]
struct Layer {
    inn: usize,
    outn: usize,
    w: Vec<f64>,
    b: Vec<f64>,
    // First-moment / velocity and second-moment accumulators (Adam/RMSProp/
    // momentum). Sized like `w`/`b`, zeroed when the optimizer changes.
    m_w: Vec<f64>,
    m_b: Vec<f64>,
    v_w: Vec<f64>,
    v_b: Vec<f64>,
}

impl Layer {
    fn new(inn: usize, outn: usize, rng: &Cell<u64>) -> Layer {
        // Xavier-ish init: small weights scaled by fan-in keeps early signals sane.
        let scale = (1.0 / inn as f64).sqrt();
        let w = (0..inn * outn).map(|_| (rand_unit(rng) * 2.0 - 1.0) * scale).collect();
        Layer {
            inn,
            outn,
            w,
            b: vec![0.0; outn],
            m_w: vec![0.0; inn * outn],
            m_b: vec![0.0; outn],
            v_w: vec![0.0; inn * outn],
            v_b: vec![0.0; outn],
        }
    }

    fn reset_state(&mut self) {
        for slot in self.m_w.iter_mut().chain(self.m_b.iter_mut()).chain(self.v_w.iter_mut()).chain(self.v_b.iter_mut()) {
            *slot = 0.0;
        }
    }
}

#[derive(Clone)]
pub struct Net {
    sizes: Vec<usize>,
    layers: Vec<Layer>,
    opt: Option<Opt>,
    t: f64,      // optimizer step count (for Adam bias correction)
    epochs: f64, // total epochs trained, for learning-rate decay
    // Where `train` runs. `None` keeps the classic per-sample (online) CPU
    // trainer; `Some(_)` switches to the batched trainer, on CPU or a GPU.
    device: Option<DeviceKind>,
}

/// A copy of one layer's weights and optimizer state, used to move a network
/// on and off a GPU (which works in `f32`, so this is where precision narrows).
pub(crate) struct LayerView {
    pub inn: usize,
    pub outn: usize,
    pub w: Vec<f64>,
    pub b: Vec<f64>,
    pub mw: Vec<f64>,
    pub vw: Vec<f64>,
    pub mb: Vec<f64>,
    pub vb: Vec<f64>,
}

impl Net {
    /// Build a network from layer sizes: `[inputs, hidden1, ..., outputs]`.
    pub fn new(sizes: Vec<usize>, seed: u64) -> Net {
        let rng = Cell::new(seed | 1); // only needed to seed the initial weights
        let layers = sizes.windows(2).map(|w| Layer::new(w[0], w[1], &rng)).collect();
        Net { sizes, layers, opt: None, t: 0.0, epochs: 0.0, device: None }
    }

    pub fn device(&self) -> Option<DeviceKind> {
        self.device
    }

    pub fn set_device(&mut self, device: Option<DeviceKind>) {
        self.device = device;
    }

    pub fn inputs(&self) -> usize {
        *self.sizes.first().unwrap_or(&0)
    }

    pub fn outputs(&self) -> usize {
        *self.sizes.last().unwrap_or(&0)
    }

    /// Forward pass, returning the activations of every layer (index 0 is the
    /// input itself, the last is the output).
    fn feed(&self, input: &[f64]) -> Vec<Vec<f64>> {
        let mut acts: Vec<Vec<f64>> = Vec::with_capacity(self.layers.len() + 1);
        acts.push(input.to_vec());
        for layer in &self.layers {
            let prev = acts.last().unwrap();
            let mut out = vec![0.0; layer.outn];
            for o in 0..layer.outn {
                let mut sum = layer.b[o];
                let base = o * layer.inn;
                for i in 0..layer.inn {
                    sum += layer.w[base + i] * prev[i];
                }
                out[o] = sigmoid(sum);
            }
            acts.push(out);
        }
        acts
    }

    /// Run the network on one input, returning its outputs.
    pub fn predict(&self, input: &[f64]) -> Vec<f64> {
        self.feed(input).pop().unwrap_or_default()
    }

    /// One epoch: a full pass over the data with online updates. Returns the
    /// mean-squared error afterward.
    pub fn train_epoch(&mut self, inputs: &[Vec<f64>], targets: &[Vec<f64>], cfg: &TrainCfg) -> f64 {
        if self.opt != Some(cfg.opt) {
            for layer in &mut self.layers {
                layer.reset_state();
            }
            self.opt = Some(cfg.opt);
            self.t = 0.0;
        }
        // Inverse-time decay: rate shrinks as training goes on (decay = 0 keeps
        // it constant).
        let rate = cfg.rate / (1.0 + cfg.decay * self.epochs);
        for (x, y) in inputs.iter().zip(targets) {
            self.t += 1.0;
            self.backprop(x, y, cfg.opt, rate);
        }
        self.epochs += 1.0;
        self.loss(inputs, targets)
    }

    /// One epoch of *batched* (full-dataset) gradient descent: accumulate the
    /// gradient over every example, then take a single averaged step. This is
    /// the algorithm the GPU runs — kept here in `f64` as the reference the GPU
    /// path is checked against, and as the CPU fallback when no GPU is found.
    pub fn train_epoch_batched(&mut self, inputs: &[Vec<f64>], targets: &[Vec<f64>], cfg: &TrainCfg) -> f64 {
        self.prepare_opt(cfg.opt);
        if inputs.is_empty() {
            return 0.0;
        }
        let rate = cfg.rate / (1.0 + cfg.decay * self.epochs);
        self.t += 1.0;

        let mut grad_w: Vec<Vec<f64>> = self.layers.iter().map(|l| vec![0.0; l.w.len()]).collect();
        let mut grad_b: Vec<Vec<f64>> = self.layers.iter().map(|l| vec![0.0; l.b.len()]).collect();
        let last = self.layers.len();
        for (x, y) in inputs.iter().zip(targets) {
            let acts = self.feed(x);
            let out = &acts[last];
            let mut delta: Vec<f64> = (0..out.len())
                .map(|o| {
                    let a = out[o];
                    let ty = y.get(o).copied().unwrap_or(0.0);
                    (a - ty) * a * (1.0 - a)
                })
                .collect();
            for l in (0..last).rev() {
                let prev = &acts[l];
                let layer = &self.layers[l];
                for o in 0..layer.outn {
                    let base = o * layer.inn;
                    for i in 0..layer.inn {
                        grad_w[l][base + i] += delta[o] * prev[i];
                    }
                    grad_b[l][o] += delta[o];
                }
                if l > 0 {
                    let mut dp = vec![0.0; layer.inn];
                    for i in 0..layer.inn {
                        let mut s = 0.0;
                        for o in 0..layer.outn {
                            s += layer.w[o * layer.inn + i] * delta[o];
                        }
                        dp[i] = s * prev[i] * (1.0 - prev[i]);
                    }
                    delta = dp;
                }
            }
        }

        const B1: f64 = 0.9;
        const B2: f64 = 0.999;
        const EPS: f64 = 1e-8;
        let inv = 1.0 / inputs.len() as f64;
        let t = self.t;
        for (l, layer) in self.layers.iter_mut().enumerate() {
            for o in 0..layer.outn {
                for i in 0..layer.inn {
                    let idx = o * layer.inn + i;
                    let g = grad_w[l][idx] * inv;
                    layer.w[idx] = step(layer.w[idx], g, cfg.opt, rate, &mut layer.m_w[idx], &mut layer.v_w[idx], t, B1, B2, EPS);
                }
                let g = grad_b[l][o] * inv;
                layer.b[o] = step(layer.b[o], g, cfg.opt, rate, &mut layer.m_b[o], &mut layer.v_b[o], t, B1, B2, EPS);
            }
        }
        self.epochs += 1.0;
        self.loss(inputs, targets)
    }

    /// Reset optimizer accumulators (and the Adam step count) when the chosen
    /// optimizer changes, so its moments don't carry over from a different rule.
    pub(crate) fn prepare_opt(&mut self, opt: Opt) {
        if self.opt != Some(opt) {
            for layer in &mut self.layers {
                layer.reset_state();
            }
            self.opt = Some(opt);
            self.t = 0.0;
        }
    }

    /// The optimizer step count and epochs-trained-so-far, used to compute the
    /// GPU's per-epoch learning-rate decay and Adam bias correction.
    pub(crate) fn progress(&self) -> (f64, f64) {
        (self.t, self.epochs)
    }

    pub(crate) fn sizes(&self) -> &[usize] {
        &self.sizes
    }

    /// Copy every layer's weights and optimizer state out, for upload to a GPU.
    pub(crate) fn snapshot(&self) -> Vec<LayerView> {
        self.layers
            .iter()
            .map(|l| LayerView {
                inn: l.inn,
                outn: l.outn,
                w: l.w.clone(),
                b: l.b.clone(),
                mw: l.m_w.clone(),
                vw: l.v_w.clone(),
                mb: l.m_b.clone(),
                vb: l.v_b.clone(),
            })
            .collect()
    }

    /// Write layer weights/state back after GPU training, and advance the
    /// step/epoch counters by however many epochs ran.
    pub(crate) fn restore(&mut self, views: Vec<LayerView>, t: f64, epochs: f64) {
        for (layer, v) in self.layers.iter_mut().zip(views) {
            layer.w = v.w;
            layer.b = v.b;
            layer.m_w = v.mw;
            layer.v_w = v.vw;
            layer.m_b = v.mb;
            layer.v_b = v.vb;
        }
        self.t = t;
        self.epochs = epochs;
    }

    /// Mean-squared error over the whole dataset.
    pub fn loss(&self, inputs: &[Vec<f64>], targets: &[Vec<f64>]) -> f64 {
        if inputs.is_empty() {
            return 0.0;
        }
        let mut total = 0.0;
        for (x, y) in inputs.iter().zip(targets) {
            let out = self.predict(x);
            for (o, t) in out.iter().zip(y) {
                let d = o - t;
                total += d * d;
            }
        }
        total / (inputs.len() * self.outputs().max(1)) as f64
    }

    /// One backward pass + weight update for a single example.
    fn backprop(&mut self, input: &[f64], target: &[f64], opt: Opt, rate: f64) {
        let acts = self.feed(input);
        let last = self.layers.len();
        // Output-layer error: dLoss/dz = (a - y) * sigmoid'(z), and sigmoid'(z)
        // = a * (1 - a) using the activation a.
        let out = &acts[last];
        let mut delta: Vec<f64> = (0..out.len())
            .map(|o| {
                let a = out[o];
                let y = target.get(o).copied().unwrap_or(0.0);
                (a - y) * a * (1.0 - a)
            })
            .collect();

        for l in (0..last).rev() {
            let prev = acts[l].clone();
            // Error to pass to the previous layer, computed from the current
            // (not-yet-updated) weights.
            let delta_prev = {
                let layer = &self.layers[l];
                let mut dp = vec![0.0; layer.inn];
                for i in 0..layer.inn {
                    let mut s = 0.0;
                    for o in 0..layer.outn {
                        s += layer.w[o * layer.inn + i] * delta[o];
                    }
                    // sigmoid' of the previous activation (unused for l == 0).
                    dp[i] = s * prev[i] * (1.0 - prev[i]);
                }
                dp
            };
            self.update(l, &delta, &prev, opt, rate);
            delta = delta_prev;
        }
    }

    /// Apply the chosen optimizer to layer `l` given its output error `delta`
    /// and the activations `prev` that fed it.
    fn update(&mut self, l: usize, delta: &[f64], prev: &[f64], opt: Opt, rate: f64) {
        const B1: f64 = 0.9;
        const B2: f64 = 0.999;
        const EPS: f64 = 1e-8;
        let t = self.t;
        let layer = &mut self.layers[l];
        for o in 0..layer.outn {
            for i in 0..layer.inn {
                let idx = o * layer.inn + i;
                let g = delta[o] * prev[i];
                layer.w[idx] = step(layer.w[idx], g, opt, rate, &mut layer.m_w[idx], &mut layer.v_w[idx], t, B1, B2, EPS);
            }
            let g = delta[o];
            layer.b[o] = step(layer.b[o], g, opt, rate, &mut layer.m_b[o], &mut layer.v_b[o], t, B1, B2, EPS);
        }
    }

    // ---- persistence -----------------------------------------------------

    /// Serialize to a small text format (inspectable, dependency-free).
    pub fn save(&self, path: &str) -> std::io::Result<()> {
        let mut out = String::from("PTNN1\n");
        let dims: Vec<String> = self.sizes.iter().map(|n| n.to_string()).collect();
        out.push_str(&dims.join(" "));
        out.push('\n');
        for layer in &self.layers {
            out.push_str(&join_floats(&layer.w));
            out.push('\n');
            out.push_str(&join_floats(&layer.b));
            out.push('\n');
        }
        std::fs::write(path, out)
    }

    /// Load a network saved by [`Net::save`]. Returns `None` if the file isn't a
    /// valid saved network.
    pub fn load(path: &str) -> Option<Net> {
        let text = std::fs::read_to_string(path).ok()?;
        let mut lines = text.lines();
        if lines.next()? != "PTNN1" {
            return None;
        }
        let sizes: Vec<usize> = lines.next()?.split_whitespace().filter_map(|s| s.parse().ok()).collect();
        if sizes.len() < 2 {
            return None;
        }
        let mut net = Net::new(sizes, 1);
        for layer in &mut net.layers {
            let w: Vec<f64> = lines.next()?.split_whitespace().filter_map(|s| s.parse().ok()).collect();
            let b: Vec<f64> = lines.next()?.split_whitespace().filter_map(|s| s.parse().ok()).collect();
            if w.len() != layer.w.len() || b.len() != layer.b.len() {
                return None;
            }
            layer.w = w;
            layer.b = b;
        }
        Some(net)
    }
}

/// A single parameter update under the chosen optimizer. Returns the new value
/// and updates the moment accumulators in place.
#[allow(clippy::too_many_arguments)]
fn step(w: f64, g: f64, opt: Opt, rate: f64, m: &mut f64, v: &mut f64, t: f64, b1: f64, b2: f64, eps: f64) -> f64 {
    match opt {
        Opt::Sgd => w - rate * g,
        Opt::Momentum => {
            *m = 0.9 * *m - rate * g;
            w + *m
        }
        Opt::RmsProp => {
            *v = 0.9 * *v + 0.1 * g * g;
            w - rate * g / (v.sqrt() + eps)
        }
        Opt::Adam => {
            *m = b1 * *m + (1.0 - b1) * g;
            *v = b2 * *v + (1.0 - b2) * g * g;
            let m_hat = *m / (1.0 - b1.powf(t));
            let v_hat = *v / (1.0 - b2.powf(t));
            w - rate * m_hat / (v_hat.sqrt() + eps)
        }
    }
}

// ---- neuroevolution ------------------------------------------------------
//
// Instead of training with backprop, breed a *population* of networks: keep the
// best, and fill the next generation with children that mix two parents' weights
// (crossover) and jiggle them a little (mutation). Fitness is whatever score the
// program measures — how far an agent got, how long it survived — so a network
// can learn to play a game without any labelled data.

impl Net {
    /// Wipe optimizer state so a bred/copied network starts clean.
    fn reset_all(&mut self) {
        for layer in &mut self.layers {
            layer.reset_state();
        }
        self.opt = None;
        self.t = 0.0;
        self.epochs = 0.0;
    }

    /// A child whose every weight/bias is taken from one of two parents at random.
    fn crossover(a: &Net, b: &Net, rng: &Cell<u64>) -> Net {
        let mut child = a.clone();
        for (l, layer) in child.layers.iter_mut().enumerate() {
            if l >= b.layers.len() {
                break;
            }
            let bl = &b.layers[l];
            for i in 0..layer.w.len().min(bl.w.len()) {
                if rand_unit(rng) < 0.5 {
                    layer.w[i] = bl.w[i];
                }
            }
            for o in 0..layer.b.len().min(bl.b.len()) {
                if rand_unit(rng) < 0.5 {
                    layer.b[o] = bl.b[o];
                }
            }
        }
        child.reset_all();
        child
    }

    /// Nudge every weight/bias by a small random amount.
    fn mutate(&mut self, strength: f64, rng: &Cell<u64>) {
        for layer in &mut self.layers {
            for w in layer.w.iter_mut().chain(layer.b.iter_mut()) {
                *w += (rand_unit(rng) * 2.0 - 1.0) * strength;
            }
        }
    }
}

/// Breed the next generation from a scored population. Returns a new set of
/// networks the same size as the input: the top `keep` carried over unchanged
/// (elitism), the rest bred from fitness-weighted parents and mutated.
pub fn evolve(nets: &[Net], scores: &[f64], mutation: f64, keep: usize, seed: u64) -> Vec<Net> {
    let n = nets.len();
    if n == 0 {
        return Vec::new();
    }
    let rng = Cell::new(seed | 1);

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(std::cmp::Ordering::Equal));

    // Roulette weights: shift so the worst still has a small chance.
    let min = scores.iter().cloned().fold(f64::INFINITY, f64::min);
    let weights: Vec<f64> = scores.iter().map(|s| (s - min) + 1e-6).collect();
    let total: f64 = weights.iter().sum();

    let mut next: Vec<Net> = Vec::with_capacity(n);
    let keep = keep.min(n);
    for &i in order.iter().take(keep) {
        let mut elite = nets[i].clone();
        elite.reset_all();
        next.push(elite);
    }
    while next.len() < n {
        let pa = roulette(&weights, total, &rng);
        let pb = roulette(&weights, total, &rng);
        let mut child = Net::crossover(&nets[pa], &nets[pb], &rng);
        child.mutate(mutation, &rng);
        next.push(child);
    }
    next
}

/// Pick an index with probability proportional to its weight.
fn roulette(weights: &[f64], total: f64, rng: &Cell<u64>) -> usize {
    let mut r = rand_unit(rng) * total;
    for (i, &w) in weights.iter().enumerate() {
        r -= w;
        if r <= 0.0 {
            return i;
        }
    }
    weights.len() - 1
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// A xorshift RNG returning a float in [0, 1). Good enough for weight init.
fn rand_unit(state: &Cell<u64>) -> f64 {
    let mut x = state.get();
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    state.set(x);
    (x >> 11) as f64 / (1u64 << 53) as f64
}

fn join_floats(xs: &[f64]) -> String {
    xs.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")
}
