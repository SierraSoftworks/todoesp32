//! E-paper display driver and renderer-agnostic frame buffer.
//!
//! [`DisplayBuffer`] is a software frame buffer that implements the
//! `embedded-graphics` [`DrawTarget`] trait, so the [`crate::controls`] can
//! draw into it without knowing anything about the hardware. [`EpdDisplay`]
//! owns the SPI bus and Waveshare 5.65" 7-colour (`OctColor`) panel and pushes
//! the buffer to the panel on demand.
//!
//! The 134 KiB frame buffer is **not** a static: it is allocated on the stack
//! for the duration of a single synchronous [`EpdDisplay::render`] call. Keeping
//! it out of `.bss` frees the corresponding DRAM, which lets the linker grow the
//! async main task's stack enough to run the (very stack-hungry) p256 TLS
//! handshake without overflowing. Rendering uses blocking SPI, so the buffer is
//! never held across an `.await` and never ends up in a task future.

use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_hal_bus::spi::ExclusiveDevice;
use epd_waveshare::color::OctColor;
use epd_waveshare::epd5in65f::{Epd5in65f, HEIGHT, WIDTH};
use epd_waveshare::graphics::DisplayRotation;
use epd_waveshare::prelude::WaveshareDisplay;
use esp_hal::Blocking;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig};
use esp_hal::peripherals::{GPIO4, GPIO5, GPIO16, GPIO17, GPIO18, GPIO23, SPI2};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;

use crate::controls::Control;

const BUFFER_LEN: usize = WIDTH as usize * HEIGHT as usize / 2;

type SpiBus = Spi<'static, Blocking>;
type SpiDev = ExclusiveDevice<SpiBus, Output<'static>, Delay>;
type Epd = Epd5in65f<SpiDev, Input<'static>, Output<'static>, Output<'static>, Delay>;

/// An error talking to the e-paper panel over SPI.
#[derive(Debug)]
pub struct DisplayError;

/// The e-paper panel and the SPI bus that drives it.
pub struct EpdDisplay {
    spi: SpiDev,
    epd: Epd,
    delay: Delay,
}

impl EpdDisplay {
    /// Initialise SPI2 and the Waveshare 5.65" (F) panel.
    ///
    /// Pin assignment matches the project README:
    /// DIN/MOSI=GPIO23, CLK/SCK=GPIO18, CS=GPIO5, DC=GPIO17, RST=GPIO16,
    /// BUSY=GPIO4.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spi: SPI2<'static>,
        sck: GPIO18<'static>,
        mosi: GPIO23<'static>,
        cs: GPIO5<'static>,
        dc: GPIO17<'static>,
        rst: GPIO16<'static>,
        busy: GPIO4<'static>,
    ) -> Result<Self, DisplayError> {
        let bus = Spi::new(
            spi,
            SpiConfig::default()
                .with_frequency(Rate::from_mhz(2))
                .with_mode(Mode::_0),
        )
        .map_err(|_| DisplayError)?
        .with_sck(sck)
        .with_mosi(mosi);

        let cs = Output::new(cs, Level::High, OutputConfig::default());
        let mut spi = ExclusiveDevice::new(bus, cs, Delay::new()).map_err(|_| DisplayError)?;

        let dc = Output::new(dc, Level::Low, OutputConfig::default());
        let rst = Output::new(rst, Level::High, OutputConfig::default());
        let busy = Input::new(busy, InputConfig::default());

        let mut delay = Delay::new();

        let mut epd =
            Epd5in65f::new(&mut spi, busy, dc, rst, &mut delay, None).map_err(|_| DisplayError)?;
        epd.set_background_color(OctColor::Black);

        Ok(Self { spi, epd, delay })
    }

    /// Draw into the frame buffer and flush the result to the panel.
    ///
    /// The frame buffer is allocated on the stack for just this call (see the
    /// module docs) and dropped before returning, so it never inflates `.bss`
    /// or a task future.
    pub fn render<R>(&mut self, render: R) -> Result<(), DisplayError>
    where
        R: FnOnce(&mut DisplayBuffer<'_>),
    {
        let mut buffer = [0u8; BUFFER_LEN];
        let mut display = DisplayBuffer::new(&mut buffer);
        render(&mut display);

        self.epd
            .wake_up(&mut self.spi, &mut self.delay)
            .map_err(|_| DisplayError)?;
        self.epd
            .update_and_display_frame(&mut self.spi, display.buffer(), &mut self.delay)
            .map_err(|_| DisplayError)?;
        self.epd
            .sleep(&mut self.spi, &mut self.delay)
            .map_err(|_| DisplayError)?;
        Ok(())
    }

    /// Re-render the panel only if at least one control reports itself dirty.
    pub fn render_controls_if_dirty(
        &mut self,
        background: OctColor,
        controls: &mut [&mut dyn Control],
    ) -> Result<(), DisplayError> {
        if !controls.iter().any(|c| c.is_dirty()) {
            return Ok(());
        }

        self.render(|d| {
            d.clear_buffer(background);
            for control in controls.iter() {
                control.render(d);
            }
        })?;

        for control in controls.iter_mut() {
            control.clear_dirty();
        }

        Ok(())
    }

    /// Put the panel controller into its lowest-power deep-sleep state.
    ///
    /// Best-effort (errors are ignored) and safe to call before the MCU itself
    /// sleeps, whether or not a frame was just rendered.
    pub fn sleep(&mut self) {
        let _ = self.epd.sleep(&mut self.spi, &mut self.delay);
    }

    pub fn width(&self) -> usize {
        WIDTH as usize
    }

    pub fn height(&self) -> usize {
        HEIGHT as usize
    }

    pub fn bounding_box(&self) -> Rectangle {
        Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT))
    }
}

/// A software frame buffer for the 7-colour panel.
///
/// Each pixel occupies a nibble (4 bits); two pixels share one byte. The buffer
/// is borrowed (not owned) so it can live on the caller's stack for just one
/// render pass — see the module docs for why that matters.
pub struct DisplayBuffer<'a> {
    buffer: &'a mut [u8; BUFFER_LEN],
    rotation: DisplayRotation,
}

impl<'a> DisplayBuffer<'a> {
    pub fn new(buffer: &'a mut [u8; BUFFER_LEN]) -> Self {
        Self {
            buffer,
            rotation: DisplayRotation::default(),
        }
    }

    pub fn width(&self) -> usize {
        WIDTH as usize
    }

    pub fn height(&self) -> usize {
        HEIGHT as usize
    }

    /// The raw packed-nibble frame buffer, ready to send to the panel.
    pub fn buffer(&self) -> &[u8] {
        self.buffer
    }

    /// Fill the entire buffer with a single background colour.
    pub fn clear_buffer(&mut self, background_color: OctColor) {
        let byte = OctColor::colors_byte(background_color, background_color);
        for cell in self.buffer.iter_mut() {
            *cell = byte;
        }
    }

    /// Set the display rotation applied to subsequent draw operations.
    pub fn set_rotation(&mut self, rotation: DisplayRotation) {
        self.rotation = rotation;
    }

    fn get_index(&self, x: usize, y: usize) -> usize {
        (y * WIDTH as usize + x) >> 1
    }

    fn effective_position(&self, x: i32, y: i32) -> Option<(usize, usize)> {
        if x < 0 || y < 0 {
            return None;
        }

        let (x, y) = match self.rotation {
            DisplayRotation::Rotate0 => (x, y),
            DisplayRotation::Rotate90 => (y, WIDTH as i32 - x - 1),
            DisplayRotation::Rotate180 => (WIDTH as i32 - x - 1, HEIGHT as i32 - y - 1),
            DisplayRotation::Rotate270 => (HEIGHT as i32 - y - 1, x),
        };

        if x >= WIDTH as i32 || y >= HEIGHT as i32 {
            return None;
        }

        Some((x as usize, y as usize))
    }
}

impl Dimensions for DisplayBuffer<'_> {
    fn bounding_box(&self) -> Rectangle {
        Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT))
    }
}

impl DrawTarget for DisplayBuffer<'_> {
    type Color = OctColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for pixel in pixels {
            if let Some((x, y)) = self.effective_position(pixel.0.x, pixel.0.y) {
                let lower = x & 0x01 != 0;
                let idx = self.get_index(x, y);

                let color = pixel.1.get_nibble() << (if lower { 0 } else { 4 });
                let mask = 0x0f << (if lower { 4 } else { 0 });

                self.buffer[idx] = (self.buffer[idx] & mask) | color;
            }
        }
        Ok(())
    }
}
