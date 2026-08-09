//! GPU acceleration for neural-network training, via `wgpu`.
//!
//! One WGSL compute path drives every desktop GPU family: `wgpu` targets Vulkan
//! and Direct3D 12 (NVIDIA / AMD / Intel) and Metal (Apple), so the vendor names
//! a program asks for — `cuda`, `rocm`, `mps` — all resolve here to a real GPU
//! adapter rather than three separate native toolkits.
//!
//! The GPU runs *batched* gradient descent in `f32`: it uploads the network's
//! weights, runs every epoch as a sequence of matrix compute kernels, then reads
//! the trained weights back. The CPU keeps an identical `f64` implementation
//! (`Net::train_epoch_batched`) that this path is numerically checked against.

use wgpu::util::DeviceExt;

use crate::nn::{Net, TrainCfg};

/// A device a program can ask a network to train on. `Cpu` means the batched
/// CPU trainer; the rest are GPU adapters selected through `wgpu`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DeviceKind {
    Cpu,
    Auto,
    Cuda,
    Rocm,
    Mps,
    Vulkan,
    Dx12,
}

impl DeviceKind {
    /// Parse the `device:` option. Vendor APIs map to the graphics backend that
    /// drives that vendor's hardware; unknown names return `None`.
    pub fn parse(name: &str) -> Option<DeviceKind> {
        match name.to_lowercase().as_str() {
            "cpu" => Some(DeviceKind::Cpu),
            "gpu" | "auto" => Some(DeviceKind::Auto),
            "cuda" | "nvidia" => Some(DeviceKind::Cuda),
            "rocm" | "hip" | "amd" => Some(DeviceKind::Rocm),
            "mps" | "metal" | "apple" => Some(DeviceKind::Mps),
            "vulkan" => Some(DeviceKind::Vulkan),
            "dx12" | "directx" | "d3d12" => Some(DeviceKind::Dx12),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            DeviceKind::Cpu => "cpu",
            DeviceKind::Auto => "gpu",
            DeviceKind::Cuda => "cuda",
            DeviceKind::Rocm => "rocm",
            DeviceKind::Mps => "mps",
            DeviceKind::Vulkan => "vulkan",
            DeviceKind::Dx12 => "dx12",
        }
    }

    fn backends(self) -> wgpu::Backends {
        use wgpu::Backends as B;
        match self {
            DeviceKind::Cpu => B::empty(),
            DeviceKind::Auto => B::all(),
            // NVIDIA and AMD GPUs are reached through Vulkan or D3D12.
            DeviceKind::Cuda | DeviceKind::Rocm | DeviceKind::Vulkan => B::VULKAN | B::DX12,
            DeviceKind::Mps => B::METAL,
            DeviceKind::Dx12 => B::DX12,
        }
    }
}

/// An opened GPU device ready to run training kernels.
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// Human description of the hardware, e.g. `AMD Radeon RX 9070 XT (Vulkan)`.
    pub info: String,
}

/// Open a GPU for the requested kind. `Err` carries a plain-language reason the
/// caller can show before falling back to the CPU.
pub fn open(kind: DeviceKind) -> Result<Gpu, String> {
    if kind == DeviceKind::Cpu {
        return Err("cpu".into());
    }
    let backends = kind.backends();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends, ..Default::default() });

    // A vendor request (cuda/rocm) must land on that vendor's hardware — asking
    // for CUDA on an AMD-only machine should fall back, not silently use the AMD
    // card. PCI vendor ids: NVIDIA 0x10DE, AMD 0x1002.
    let want_vendor: Option<u32> = match kind {
        DeviceKind::Cuda => Some(0x10DE),
        DeviceKind::Rocm => Some(0x1002),
        _ => None,
    };

    // Prefer a real GPU; skip software rasterizers so "gpu" means a GPU.
    let mut adapters: Vec<wgpu::Adapter> = instance.enumerate_adapters(backends);
    adapters.retain(|a| {
        let info = a.get_info();
        info.device_type != wgpu::DeviceType::Cpu && want_vendor.map_or(true, |v| info.vendor == v)
    });
    if adapters.is_empty() {
        return Err(format!("no {} GPU found", kind.label()));
    }
    adapters.sort_by_key(|a| match a.get_info().device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        _ => 3,
    });
    let adapter = &adapters[0];
    let ainfo = adapter.get_info();

    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("plaintext-nn"),
            ..Default::default()
        },
        None,
    ))
    .map_err(|e| format!("couldn't open a {} device: {}", kind.label(), e))?;

    Ok(Gpu {
        device,
        queue,
        info: format!("{} ({:?})", ainfo.name, ainfo.backend),
    })
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Dims {
    n: u32,
    inn: u32,
    outn: u32,
    opt: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Hyper {
    rate: f32,
    t: f32,
    b1: f32,
    b2: f32,
    eps: f32,
    _pad: [f32; 3],
}

/// Shared WGSL prelude: the parameter blocks and the sigmoid used by kernels.
const PRELUDE: &str = r#"
struct Dims  { n: u32, inn: u32, outn: u32, opt: u32 };
struct Hyper { rate: f32, t: f32, b1: f32, b2: f32, eps: f32, p0: f32, p1: f32, p2: f32 };
fn sig(x: f32) -> f32 { return 1.0 / (1.0 + exp(-x)); }
"#;

// a_out[s,o] = sigmoid( b[o] + sum_i a_in[s,i] * w[o,i] )
const FORWARD: &str = r#"
@group(0) @binding(0) var<uniform> d: Dims;
@group(0) @binding(1) var<storage, read>       a_in:  array<f32>;
@group(0) @binding(2) var<storage, read>       w:     array<f32>;
@group(0) @binding(3) var<storage, read>       b:     array<f32>;
@group(0) @binding(4) var<storage, read_write> a_out: array<f32>;
@compute @workgroup_size(64)
fn forward(@builtin(global_invocation_id) g: vec3<u32>) {
    let idx = g.x;
    if (idx >= d.n * d.outn) { return; }
    let s = idx / d.outn; let o = idx % d.outn;
    var sum = b[o];
    let ab = s * d.inn; let wb = o * d.inn;
    for (var i: u32 = 0u; i < d.inn; i = i + 1u) { sum = sum + w[wb + i] * a_in[ab + i]; }
    a_out[idx] = sig(sum);
}
"#;

// d_out[s,o] = (a - y) * a * (1 - a)   (output-layer error)
const OUT_DELTA: &str = r#"
@group(0) @binding(0) var<uniform> d: Dims;
@group(0) @binding(1) var<storage, read>       a_out: array<f32>;
@group(0) @binding(2) var<storage, read>       y:     array<f32>;
@group(0) @binding(3) var<storage, read_write> d_out: array<f32>;
@compute @workgroup_size(64)
fn out_delta(@builtin(global_invocation_id) g: vec3<u32>) {
    let idx = g.x;
    if (idx >= d.n * d.outn) { return; }
    let a = a_out[idx];
    d_out[idx] = (a - y[idx]) * a * (1.0 - a);
}
"#;

// dw[o,i] = (1/n) sum_s dd[s,o] * a_in[s,i]
const GRAD_W: &str = r#"
@group(0) @binding(0) var<uniform> d: Dims;
@group(0) @binding(1) var<storage, read>       a_in: array<f32>;
@group(0) @binding(2) var<storage, read>       dd:   array<f32>;
@group(0) @binding(3) var<storage, read_write> dw:   array<f32>;
@compute @workgroup_size(64)
fn grad_w(@builtin(global_invocation_id) g: vec3<u32>) {
    let idx = g.x;
    if (idx >= d.outn * d.inn) { return; }
    let o = idx / d.inn; let i = idx % d.inn;
    var acc = 0.0;
    for (var s: u32 = 0u; s < d.n; s = s + 1u) { acc = acc + dd[s * d.outn + o] * a_in[s * d.inn + i]; }
    dw[idx] = acc / f32(d.n);
}
"#;

// db[o] = (1/n) sum_s dd[s,o]
const GRAD_B: &str = r#"
@group(0) @binding(0) var<uniform> d: Dims;
@group(0) @binding(1) var<storage, read>       dd: array<f32>;
@group(0) @binding(2) var<storage, read_write> db: array<f32>;
@compute @workgroup_size(64)
fn grad_b(@builtin(global_invocation_id) g: vec3<u32>) {
    let o = g.x;
    if (o >= d.outn) { return; }
    var acc = 0.0;
    for (var s: u32 = 0u; s < d.n; s = s + 1u) { acc = acc + dd[s * d.outn + o]; }
    db[o] = acc / f32(d.n);
}
"#;

// d_prev[s,i] = ( sum_o dd[s,o] * w[o,i] ) * a_prev[s,i] * (1 - a_prev[s,i])
const BACK_DELTA: &str = r#"
@group(0) @binding(0) var<uniform> d: Dims;
@group(0) @binding(1) var<storage, read>       dd:     array<f32>;
@group(0) @binding(2) var<storage, read>       w:      array<f32>;
@group(0) @binding(3) var<storage, read>       a_prev: array<f32>;
@group(0) @binding(4) var<storage, read_write> d_prev: array<f32>;
@compute @workgroup_size(64)
fn back_delta(@builtin(global_invocation_id) g: vec3<u32>) {
    let idx = g.x;
    if (idx >= d.n * d.inn) { return; }
    let s = idx / d.inn; let i = idx % d.inn;
    var acc = 0.0;
    for (var o: u32 = 0u; o < d.outn; o = o + 1u) { acc = acc + dd[s * d.outn + o] * w[o * d.inn + i]; }
    let a = a_prev[idx];
    d_prev[idx] = acc * a * (1.0 - a);
}
"#;

// One optimizer step over a weight (or bias) array, mirroring nn::step exactly.
const OPT: &str = r#"
@group(0) @binding(0) var<uniform> d: Dims;
@group(0) @binding(1) var<uniform> h: Hyper;
@group(0) @binding(2) var<storage, read_write> w:  array<f32>;
@group(0) @binding(3) var<storage, read>       gr: array<f32>;
@group(0) @binding(4) var<storage, read_write> m:  array<f32>;
@group(0) @binding(5) var<storage, read_write> v:  array<f32>;
fn apply(idx: u32, total: u32) {
    if (idx >= total) { return; }
    let g = gr[idx];
    var mv = m[idx]; var vv = v[idx]; var wv = w[idx];
    if (d.opt == 0u) {
        wv = wv - h.rate * g;
    } else if (d.opt == 1u) {
        mv = 0.9 * mv - h.rate * g; wv = wv + mv;
    } else if (d.opt == 2u) {
        vv = 0.9 * vv + 0.1 * g * g; wv = wv - h.rate * g / (sqrt(vv) + h.eps);
    } else {
        mv = h.b1 * mv + (1.0 - h.b1) * g;
        vv = h.b2 * vv + (1.0 - h.b2) * g * g;
        let mh = mv / (1.0 - pow(h.b1, h.t));
        let vh = vv / (1.0 - pow(h.b2, h.t));
        wv = wv - h.rate * mh / (sqrt(vh) + h.eps);
    }
    m[idx] = mv; v[idx] = vv; w[idx] = wv;
}
@compute @workgroup_size(64)
fn opt_w(@builtin(global_invocation_id) g: vec3<u32>) { apply(g.x, d.outn * d.inn); }
@compute @workgroup_size(64)
fn opt_b(@builtin(global_invocation_id) g: vec3<u32>) { apply(g.x, d.outn); }
"#;

/// GPU buffers and bind groups for one layer, sized to the current dataset.
struct GpuLayer {
    inn: usize,
    outn: usize,
    w: wgpu::Buffer,
    b: wgpu::Buffer,
    mw: wgpu::Buffer,
    vw: wgpu::Buffer,
    mb: wgpu::Buffer,
    vb: wgpu::Buffer,
    dw: wgpu::Buffer,
    db: wgpu::Buffer,
    dims: wgpu::Buffer,
}

/// Train `net` on `gpu` for `epochs` epochs of batched gradient descent, then
/// copy the trained weights back into `net`. Returns the final loss.
pub fn train(
    gpu: &Gpu,
    net: &mut Net,
    cfg: &TrainCfg,
    inputs: &[Vec<f64>],
    targets: &[Vec<f64>],
    epochs: u64,
) -> Result<f64, String> {
    if inputs.is_empty() || epochs == 0 {
        return Ok(net.loss(inputs, targets));
    }
    net.prepare_opt(cfg.opt);
    let (t0, e0) = net.progress();
    let snap = net.snapshot();
    let sizes: Vec<usize> = net.sizes().to_vec();
    let n = inputs.len();
    let dev = &gpu.device;

    let pipelines = Pipelines::new(dev);

    // Flatten the dataset (row-major) into f32 for the GPU.
    let x_flat: Vec<f32> = inputs.iter().flatten().map(|&v| v as f32).collect();
    let y_flat: Vec<f32> = targets.iter().flatten().map(|&v| v as f32).collect();

    let storage = wgpu::BufferUsages::STORAGE;
    let readable = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC;

    // acts[0] = inputs; acts[l+1] = layer l's output activations.
    let mut acts: Vec<wgpu::Buffer> = Vec::with_capacity(sizes.len());
    acts.push(init_buf(dev, "x", &x_flat, storage));
    for &out in &sizes[1..] {
        acts.push(zeros_buf(dev, "a", n * out, storage));
    }
    // delt[l] = error at layer l's output.
    let delt: Vec<wgpu::Buffer> = sizes[1..].iter().map(|&out| zeros_buf(dev, "d", n * out, storage)).collect();
    let y_buf = init_buf(dev, "y", &y_flat, storage);

    let opt_id = cfg.opt.id();
    let layers: Vec<GpuLayer> = snap
        .iter()
        .map(|lv| GpuLayer {
            inn: lv.inn,
            outn: lv.outn,
            w: init_buf(dev, "w", &to_f32(&lv.w), readable),
            b: init_buf(dev, "b", &to_f32(&lv.b), readable),
            mw: init_buf(dev, "mw", &to_f32(&lv.mw), readable),
            vw: init_buf(dev, "vw", &to_f32(&lv.vw), readable),
            mb: init_buf(dev, "mb", &to_f32(&lv.mb), readable),
            vb: init_buf(dev, "vb", &to_f32(&lv.vb), readable),
            dw: zeros_buf(dev, "dw", lv.w.len(), storage),
            db: zeros_buf(dev, "db", lv.b.len(), storage),
            dims: init_uniform(dev, Dims { n: n as u32, inn: lv.inn as u32, outn: lv.outn as u32, opt: opt_id }),
        })
        .collect();

    let hyper = dev.create_buffer(&wgpu::BufferDescriptor {
        label: Some("hyper"),
        size: std::mem::size_of::<Hyper>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Precreate every bind group (nothing but `hyper` changes between epochs).
    let l_count = layers.len();
    let fwd_bg: Vec<_> = (0..l_count)
        .map(|l| bind(dev, &pipelines.forward, &[u(&layers[l].dims), s(&acts[l]), s(&layers[l].w), s(&layers[l].b), s(&acts[l + 1])]))
        .collect();
    let out_delta_bg = bind(dev, &pipelines.out_delta, &[u(&layers[l_count - 1].dims), s(&acts[l_count]), s(&y_buf), s(&delt[l_count - 1])]);
    let grad_w_bg: Vec<_> = (0..l_count)
        .map(|l| bind(dev, &pipelines.grad_w, &[u(&layers[l].dims), s(&acts[l]), s(&delt[l]), s(&layers[l].dw)]))
        .collect();
    let grad_b_bg: Vec<_> = (0..l_count)
        .map(|l| bind(dev, &pipelines.grad_b, &[u(&layers[l].dims), s(&delt[l]), s(&layers[l].db)]))
        .collect();
    let back_bg: Vec<_> = (1..l_count)
        .map(|l| bind(dev, &pipelines.back_delta, &[u(&layers[l].dims), s(&delt[l]), s(&layers[l].w), s(&acts[l]), s(&delt[l - 1])]))
        .collect();
    let opt_w_bg: Vec<_> = (0..l_count)
        .map(|l| bind(dev, &pipelines.opt_w, &[u(&layers[l].dims), u(&hyper), s(&layers[l].w), s(&layers[l].dw), s(&layers[l].mw), s(&layers[l].vw)]))
        .collect();
    let opt_b_bg: Vec<_> = (0..l_count)
        .map(|l| bind(dev, &pipelines.opt_b, &[u(&layers[l].dims), u(&hyper), s(&layers[l].b), s(&layers[l].db), s(&layers[l].mb), s(&layers[l].vb)]))
        .collect();

    for e in 0..epochs {
        let rate = cfg.rate / (1.0 + cfg.decay * (e0 + e as f64));
        let h = Hyper { rate: rate as f32, t: (t0 + e as f64 + 1.0) as f32, b1: 0.9, b2: 0.999, eps: 1e-8, _pad: [0.0; 3] };
        gpu.queue.write_buffer(&hyper, 0, bytemuck::bytes_of(&h));

        let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("epoch") });
        for l in 0..l_count {
            pass(&mut enc, &pipelines.forward, &fwd_bg[l], groups(n * layers[l].outn));
        }
        pass(&mut enc, &pipelines.out_delta, &out_delta_bg, groups(n * layers[l_count - 1].outn));
        for l in (0..l_count).rev() {
            pass(&mut enc, &pipelines.grad_w, &grad_w_bg[l], groups(layers[l].outn * layers[l].inn));
            pass(&mut enc, &pipelines.grad_b, &grad_b_bg[l], groups(layers[l].outn));
            if l > 0 {
                pass(&mut enc, &pipelines.back_delta, &back_bg[l - 1], groups(n * layers[l].inn));
            }
            pass(&mut enc, &pipelines.opt_w, &opt_w_bg[l], groups(layers[l].outn * layers[l].inn));
            pass(&mut enc, &pipelines.opt_b, &opt_b_bg[l], groups(layers[l].outn));
        }
        gpu.queue.submit(Some(enc.finish()));
    }

    // Read trained weights + optimizer state back into the network.
    let mut views = snap;
    for (l, lv) in views.iter_mut().enumerate() {
        lv.w = from_f32(&read_buf(dev, &gpu.queue, &layers[l].w, lv.w.len()));
        lv.b = from_f32(&read_buf(dev, &gpu.queue, &layers[l].b, lv.b.len()));
        lv.mw = from_f32(&read_buf(dev, &gpu.queue, &layers[l].mw, lv.mw.len()));
        lv.vw = from_f32(&read_buf(dev, &gpu.queue, &layers[l].vw, lv.vw.len()));
        lv.mb = from_f32(&read_buf(dev, &gpu.queue, &layers[l].mb, lv.mb.len()));
        lv.vb = from_f32(&read_buf(dev, &gpu.queue, &layers[l].vb, lv.vb.len()));
    }
    net.restore(views, t0 + epochs as f64, e0 + epochs as f64);
    Ok(net.loss(inputs, targets))
}

/// The seven compute pipelines, built once per training run.
struct Pipelines {
    forward: wgpu::ComputePipeline,
    out_delta: wgpu::ComputePipeline,
    grad_w: wgpu::ComputePipeline,
    grad_b: wgpu::ComputePipeline,
    back_delta: wgpu::ComputePipeline,
    opt_w: wgpu::ComputePipeline,
    opt_b: wgpu::ComputePipeline,
}

impl Pipelines {
    fn new(dev: &wgpu::Device) -> Pipelines {
        Pipelines {
            forward: pipeline(dev, FORWARD, "forward"),
            out_delta: pipeline(dev, OUT_DELTA, "out_delta"),
            grad_w: pipeline(dev, GRAD_W, "grad_w"),
            grad_b: pipeline(dev, GRAD_B, "grad_b"),
            back_delta: pipeline(dev, BACK_DELTA, "back_delta"),
            opt_w: pipeline(dev, OPT, "opt_w"),
            opt_b: pipeline(dev, OPT, "opt_b"),
        }
    }
}

fn pipeline(dev: &wgpu::Device, body: &str, entry: &str) -> wgpu::ComputePipeline {
    let module = dev.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(entry),
        source: wgpu::ShaderSource::Wgsl(format!("{PRELUDE}{body}").into()),
    });
    dev.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(entry),
        layout: None,
        module: &module,
        entry_point: entry,
        compilation_options: Default::default(),
        cache: None,
    })
}

fn pass(enc: &mut wgpu::CommandEncoder, pipe: &wgpu::ComputePipeline, bg: &wgpu::BindGroup, groups: u32) {
    let mut p = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: None });
    p.set_pipeline(pipe);
    p.set_bind_group(0, bg, &[]);
    p.dispatch_workgroups(groups, 1, 1);
}

fn groups(total: usize) -> u32 {
    ((total as u32) + 63) / 64
}

fn u(buf: &wgpu::Buffer) -> wgpu::BindingResource<'_> {
    buf.as_entire_binding()
}
fn s(buf: &wgpu::Buffer) -> wgpu::BindingResource<'_> {
    buf.as_entire_binding()
}

fn bind(dev: &wgpu::Device, pipe: &wgpu::ComputePipeline, resources: &[wgpu::BindingResource]) -> wgpu::BindGroup {
    let entries: Vec<wgpu::BindGroupEntry> = resources
        .iter()
        .enumerate()
        .map(|(i, r)| wgpu::BindGroupEntry { binding: i as u32, resource: r.clone() })
        .collect();
    dev.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipe.get_bind_group_layout(0),
        entries: &entries,
    })
}

fn init_buf(dev: &wgpu::Device, label: &str, data: &[f32], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(data),
        usage,
    })
}

fn zeros_buf(dev: &wgpu::Device, label: &str, len: usize, usage: wgpu::BufferUsages) -> wgpu::Buffer {
    init_buf(dev, label, &vec![0.0f32; len.max(1)], usage)
}

fn init_uniform(dev: &wgpu::Device, dims: Dims) -> wgpu::Buffer {
    dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("dims"),
        contents: bytemuck::bytes_of(&dims),
        usage: wgpu::BufferUsages::UNIFORM,
    })
}

/// Copy a storage buffer back to the CPU and read it as `f32`s.
fn read_buf(dev: &wgpu::Device, queue: &wgpu::Queue, buf: &wgpu::Buffer, len: usize) -> Vec<f32> {
    let bytes = (len * 4) as u64;
    let staging = dev.create_buffer(&wgpu::BufferDescriptor {
        label: Some("read"),
        size: bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    enc.copy_buffer_to_buffer(buf, 0, &staging, 0, bytes);
    queue.submit(Some(enc.finish()));

    let slice = staging.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    dev.poll(wgpu::Maintain::Wait);
    let data = slice.get_mapped_range();
    bytemuck::cast_slice(&data).to_vec()
}

fn to_f32(xs: &[f64]) -> Vec<f32> {
    xs.iter().map(|&x| x as f32).collect()
}

fn from_f32(xs: &[f32]) -> Vec<f64> {
    xs.iter().map(|&x| x as f64).collect()
}
