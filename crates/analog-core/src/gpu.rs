//! GPU context: a `wgpu` device and queue, plus upload/readback helpers.
//!
//! The engine keeps a single long-lived device. Adapters are requested with a
//! high-performance preference; if no adapter is available, [`GpuContext::new`]
//! returns [`Error::NoAdapter`] so callers (and tests) can skip GPU work
//! gracefully on machines without a usable GPU.

use crate::error::{Error, Result};

/// An initialised `wgpu` device and its queue.
pub struct GpuContext {
    /// The logical device.
    pub device: wgpu::Device,
    /// The submission queue for the device.
    pub queue: wgpu::Queue,
    /// Human-readable information about the chosen adapter.
    pub adapter_info: wgpu::AdapterInfo,
}

impl GpuContext {
    /// Create a context asynchronously.
    ///
    /// Returns [`Error::NoAdapter`] when no compatible adapter exists and
    /// [`Error::Gpu`] when the device request fails.
    pub async fn new() -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                ..Default::default()
            })
            .await
            .map_err(|e| Error::NoAdapter(e.to_string()))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("analog-core"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| Error::Gpu(e.to_string()))?;

        let adapter_info = adapter.get_info();
        log::debug!(
            "gpu adapter: {} ({:?}, {:?})",
            adapter_info.name,
            adapter_info.backend,
            adapter_info.device_type
        );
        Ok(Self {
            device,
            queue,
            adapter_info,
        })
    }

    /// Blocking wrapper around [`GpuContext::new`].
    pub fn new_blocking() -> Result<Self> {
        pollster::block_on(Self::new())
    }

    /// The maximum number of pixels the adapter's 2D texture limit allows.
    pub fn max_pixels(&self) -> u64 {
        let limit = self.device.limits().max_texture_dimension_2d as u64;
        limit * limit
    }
}
