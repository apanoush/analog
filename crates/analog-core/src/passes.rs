//! GPU compute passes.
//!
//! [`GpuImage`] is a pixel buffer that lives on the GPU. [`ImageComputePass`]
//! runs a WGSL compute entry point over one or more `GpuImage` inputs and
//! returns a new `GpuImage`, so a chain of passes stays on the GPU with a single
//! readback at the end (see [`GpuImage::read`]). This replaced an earlier design
//! that read back to the CPU after every pass.
//!
//! Binding convention for every shader built on it:
//!
//! - bindings `0..n` are read-only `array<u32>` storage inputs,
//! - binding `n` is the read-write output,
//! - binding `n + 1` is the uniform parameter block.

use std::sync::OnceLock;

use wgpu::util::DeviceExt;

use crate::error::{Error, Result};
use crate::gpu::GpuContext;
use crate::image::{Image, PixelFormat};
use crate::shaders::image_shader;

/// A pixel buffer resident on the GPU.
pub struct GpuImage {
    buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    format: PixelFormat,
}

impl GpuImage {
    /// Upload a CPU image to a GPU buffer.
    pub fn upload(ctx: &GpuContext, image: &Image) -> Result<Self> {
        let words = image.as_words();
        let buffer = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("gpu_image"),
                contents: bytemuck::cast_slice(&words),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            });
        Ok(Self {
            buffer,
            width: image.width(),
            height: image.height(),
            format: image.format(),
        })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Pixel layout.
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// Bytes in the buffer.
    fn byte_len(&self) -> u64 {
        self.width as u64 * self.height as u64 * self.format.bytes_per_pixel() as u64
    }

    /// Read the buffer back to a CPU image. This is the only synchronisation
    /// point in a render.
    pub fn read(&self, ctx: &GpuContext) -> Result<Image> {
        let len = self.byte_len();
        let staging = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_image.readback"),
            size: len,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu_image.readback"),
            });
        encoder.copy_buffer_to_buffer(&self.buffer, 0, &staging, 0, len);
        ctx.queue.submit(Some(encoder.finish()));

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            let _ = tx.send(res);
        });
        ctx.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| Error::Gpu(e.to_string()))?;
        rx.recv()
            .map_err(|e| Error::Gpu(e.to_string()))?
            .map_err(|e| Error::Gpu(e.to_string()))?;

        let data = {
            let view = slice
                .get_mapped_range()
                .map_err(|e| Error::Gpu(e.to_string()))?;
            view.to_vec()
        };
        staging.unmap();

        Image::from_bytes(self.width, self.height, self.format, data)
    }
}

/// A compiled compute pass with the input/output/uniform binding convention.
pub struct ImageComputePass {
    pipeline: wgpu::ComputePipeline,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl ImageComputePass {
    /// Compile a WGSL module and build its compute pipeline.
    pub fn new(ctx: &GpuContext, label: &str, wgsl: &str, entry: &str) -> Self {
        let shader = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(wgsl.into()),
            });
        let pipeline = ctx
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: None,
                module: &shader,
                entry_point: Some(entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
        let bind_group_layout = pipeline.get_bind_group_layout(0);
        Self {
            pipeline,
            bind_group_layout,
        }
    }

    /// Run the pass over `inputs`, producing a GPU image of `out_w` x `out_h` in
    /// `out_format`. No readback happens here.
    pub fn run_gpu(
        &self,
        ctx: &GpuContext,
        inputs: &[&GpuImage],
        out_w: u32,
        out_h: u32,
        out_format: PixelFormat,
        params: &[u8],
    ) -> Result<GpuImage> {
        let n = inputs.len();
        if n == 0 {
            return Err(Error::Gpu("a pass needs at least one input".into()));
        }

        let out_len = out_w as u64 * out_h as u64 * out_format.bytes_per_pixel() as u64;
        let dst = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pass.dst"),
            size: out_len,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let params_size = align_up((params.len() as u64).max(16), 16);
        let mut params_bytes = params.to_vec();
        params_bytes.resize(params_size as usize, 0);
        let params_buf = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("pass.params"),
                contents: &params_bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let mut entries: Vec<wgpu::BindGroupEntry> = Vec::with_capacity(n + 2);
        for (i, img) in inputs.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: i as u32,
                resource: img.buffer.as_entire_binding(),
            });
        }
        entries.push(wgpu::BindGroupEntry {
            binding: n as u32,
            resource: dst.as_entire_binding(),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: (n + 1) as u32,
            resource: params_buf.as_entire_binding(),
        });

        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pass.bind_group"),
            layout: &self.bind_group_layout,
            entries: &entries,
        });

        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pass.encoder"),
            });
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("pass"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(out_w.div_ceil(16), out_h.div_ceil(16), 1);
        }
        ctx.queue.submit(Some(encoder.finish()));

        Ok(GpuImage {
            buffer: dst,
            width: out_w,
            height: out_h,
            format: out_format,
        })
    }

    /// Run the pass over a single CPU image and read the result back. Convenient
    /// for tests and one-off use; the render pipeline uses [`Self::run_gpu`].
    pub fn run(&self, ctx: &GpuContext, input: &Image, params: &[u8]) -> Result<Image> {
        let gpu = GpuImage::upload(ctx, input)?;
        self.run_gpu(
            ctx,
            &[&gpu],
            input.width(),
            input.height(),
            input.format(),
            params,
        )?
        .read(ctx)
    }
}

/// A pass compiled on first use, so startup does not pay for unused pipelines.
pub struct LazyPass {
    label: &'static str,
    wgsl: &'static str,
    cell: OnceLock<ImageComputePass>,
}

impl LazyPass {
    /// Create a lazy pass from a WGSL module.
    pub fn new(label: &'static str, wgsl: &'static str) -> Self {
        Self {
            label,
            wgsl,
            cell: OnceLock::new(),
        }
    }

    /// Compile (once) and return the pass.
    pub fn get(&self, ctx: &GpuContext) -> &ImageComputePass {
        self.cell
            .get_or_init(|| ImageComputePass::new(ctx, self.label, self.wgsl, "main"))
    }
}

/// A pass that reads or writes the image, compiled on first use for one
/// [`PixelFormat`]. Only the format actually used is ever compiled.
pub struct ImagePassPair {
    label: &'static str,
    wgsl: &'static str,
    u8: OnceLock<ImageComputePass>,
    u16: OnceLock<ImageComputePass>,
}

impl ImagePassPair {
    /// Create a lazy image pass from a WGSL module using the image markers.
    pub fn new(label: &'static str, wgsl: &'static str) -> Self {
        Self {
            label,
            wgsl,
            u8: OnceLock::new(),
            u16: OnceLock::new(),
        }
    }

    /// Compile (once) and return the variant for the given format.
    pub fn select(&self, ctx: &GpuContext, format: PixelFormat) -> &ImageComputePass {
        match format {
            PixelFormat::Rgba8 => self.u8.get_or_init(|| {
                ImageComputePass::new(ctx, self.label, &image_shader(self.wgsl, format), "main")
            }),
            PixelFormat::Rgba16 => self.u16.get_or_init(|| {
                ImageComputePass::new(ctx, self.label, &image_shader(self.wgsl, format), "main")
            }),
        }
    }
}

/// P2 plumbing pass: RGBA8 to BT.709 luma, alpha preserved.
///
/// Proves upload, dispatch and readback. Not part of the final effect pipeline.
pub fn grayscale(ctx: &GpuContext, input: &Image) -> Result<Image> {
    let pass = ImageComputePass::new(
        ctx,
        "grayscale",
        include_str!("../shaders/grayscale.wgsl"),
        "main",
    );
    let words = [input.width(), input.height(), 0u32, 0u32];
    pass.run(ctx, input, bytemuck::cast_slice(&words))
}

fn align_up(value: u64, alignment: u64) -> u64 {
    value.div_ceil(alignment) * alignment
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CPU reference for the grayscale pass, matching `shaders/grayscale.wgsl`.
    fn cpu_grayscale(input: &Image) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.data().len());
        for px in input.data().chunks_exact(4) {
            let r = px[0] as f32 / 255.0;
            let g = px[1] as f32 / 255.0;
            let b = px[2] as f32 / 255.0;
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let l = (luma * 255.0).round().clamp(0.0, 255.0) as u8;
            out.extend_from_slice(&[l, l, l, px[3]]);
        }
        out
    }

    #[test]
    fn grayscale_matches_cpu_reference() {
        let ctx = match GpuContext::new_blocking() {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!("skipping GPU test: {e}");
                return;
            }
        };
        // A 33x17 image with a non-trivial pattern (odd size to exercise the
        // partial workgroup edge).
        let (w, h) = (33u32, 17u32);
        let data: Vec<u8> = (0..w * h * 4).map(|i| (i * 37 + 11) as u8).collect();
        let input = Image::from_rgba8(w, h, data).unwrap();

        let gpu = grayscale(&ctx, &input).unwrap();
        let cpu = cpu_grayscale(&input);

        assert_eq!(gpu.width(), w);
        assert_eq!(gpu.height(), h);
        for (i, (a, b)) in gpu.data().iter().zip(cpu.iter()).enumerate() {
            assert!(a.abs_diff(*b) <= 1, "pixel byte {i}: gpu={a} cpu={b}");
        }
    }

    #[test]
    fn gpu_image_roundtrips() {
        let ctx = match GpuContext::new_blocking() {
            Ok(ctx) => ctx,
            Err(e) => {
                eprintln!("skipping GPU test: {e}");
                return;
            }
        };
        let img = Image::from_rgba8(4, 3, (0..48).map(|i| (i * 5) as u8).collect()).unwrap();
        let gpu = GpuImage::upload(&ctx, &img).unwrap();
        assert_eq!(gpu.read(&ctx).unwrap(), img);
    }

    #[test]
    fn align_up_rounds_to_multiple() {
        assert_eq!(align_up(0, 16), 0);
        assert_eq!(align_up(1, 16), 16);
        assert_eq!(align_up(16, 16), 16);
        assert_eq!(align_up(17, 16), 32);
    }
}
