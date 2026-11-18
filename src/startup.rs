//! A software-drawn startup window, independent of the simulator's GPU.
use std::{num::NonZeroU32, sync::Arc, time::{Duration, Instant}};
use winit::{dpi::LogicalSize, event_loop::ActiveEventLoop, window::{Window, WindowLevel}};

pub struct Splash {
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    pub window: Arc<Window>,
    art: image::RgbImage,
    pub deadline: Instant,
    painted: bool,
}

impl Splash {
    pub fn new(event_loop: &ActiveEventLoop, title: &str) -> anyhow::Result<Self> {
        let art = image::load_from_memory(include_bytes!("../assets/ui/startup.jpg"))?.into_rgb8();
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(640.0, 360.0))
            .with_resizable(false)
            .with_window_level(WindowLevel::AlwaysOnTop);
        let window = Arc::new(event_loop.create_window(attrs)?);
        let context = softbuffer::Context::new(window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Self { surface, window, art, deadline: Instant::now() + Duration::from_millis(300), painted: false })
    }

    pub fn paint(&mut self) -> Result<(), softbuffer::SoftBufferError> {
        let size = self.window.inner_size();
        let (Some(width), Some(height)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return Ok(()) };
        self.surface.resize(width, height)?;
        let pixels = image::imageops::resize(&self.art, width.get(), height.get(), image::imageops::FilterType::Triangle);
        let mut buffer = self.surface.buffer_mut()?;
        for (dst, pixel) in buffer.iter_mut().zip(pixels.pixels()) {
            let [r, g, b] = pixel.0.map(u32::from);
            *dst = (r << 16) | (g << 8) | b;
        }
        self.window.pre_present_notify();
        buffer.present()?;
        if !self.painted {
            self.painted = true;
            // Give the compositor a turn to display the software frame before
            // any main-thread work for the simulator begins.
            self.deadline = Instant::now() + Duration::from_millis(50);
        }
        Ok(())
    }
}
