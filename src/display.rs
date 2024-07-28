use std::ptr::addr_of_mut;

use embedded_graphics::prelude::*;
use epd_waveshare::graphics::*;
use epd_waveshare::prelude::*;
use epd_waveshare::epd5in65f::*;
use esp_idf_svc::hal::*;
use esp_idf_svc::hal::prelude::*;

use crate::controls::Control;

pub type DisplayDriver = spi::SpiDeviceDriver<'static, spi::SpiDriver<'static>>;

pub type EPDisplay<D> = Epd5in65f<
    DisplayDriver,
    gpio::PinDriver<'static, gpio::AnyOutputPin, gpio::Output>,
    gpio::PinDriver<'static, gpio::AnyInputPin, gpio::Input>,
    gpio::PinDriver<'static, gpio::AnyOutputPin, gpio::Output>,
    gpio::PinDriver<'static, gpio::AnyOutputPin, gpio::Output>,
    D,
>;

pub type DisplayType = DisplayBuffer;
static mut BUFFER: [u8; WIDTH as usize * HEIGHT as usize / 2] = [0; WIDTH as usize * HEIGHT as usize / 2];

pub struct Display<D: embedded_hal::blocking::delay::DelayMs<u8>> {
    driver: DisplayDriver,
    epd: EPDisplay<D>,
    display: DisplayBuffer,
    delay: D,
}

impl<D: embedded_hal::blocking::delay::DelayMs<u8>> Display<D> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spi: impl peripheral::Peripheral<P = impl spi::SpiAnyPins> + 'static,
        din: gpio::AnyOutputPin,
        clk: gpio::AnyOutputPin,
        cs: gpio::AnyOutputPin,
        dc: gpio::AnyOutputPin,
        rst: gpio::AnyOutputPin,
        busy: gpio::AnyInputPin,
        mut delay: D,
    ) -> anyhow::Result<Self> {
        let mut display_driver = spi::SpiDeviceDriver::new_single(
            spi,
            clk,
            din,
            Option::<gpio::AnyIOPin>::None,
            Option::<gpio::AnyOutputPin>::None,
            &spi::SpiDriverConfig::new().dma(spi::Dma::Disabled),
            &spi::SpiConfig::new().baudrate(2.MHz().into()).write_only(true).bit_order(spi::config::BitOrder::MsbFirst).data_mode(spi::config::MODE_0),
        )?;
    
        log::info!("Configuring EPD driver");
        let mut epd = EPDisplay::new(
            &mut display_driver,
            gpio::PinDriver::output(cs)?,
            gpio::PinDriver::input(busy)?,
            gpio::PinDriver::output(dc)?,
            gpio::PinDriver::output(rst)?,
            &mut delay,
        )?;

        epd.set_background_color(OctColor::Black);
    
        log::info!("Creating display buffer");
        let display = DisplayBuffer::new();
        
        Ok(Self {
            driver: display_driver,
            epd,
            display,
            delay,
        })
    }

    pub fn render<R>(&mut self, render: R) -> anyhow::Result<()>
    where
        R: FnOnce(&mut DisplayType) -> anyhow::Result<()>,
    {
        render(&mut self.display)?;
        self.epd.wake_up(&mut self.driver, &mut self.delay)?;
        self.epd.update_and_display_frame(&mut self.driver, self.display.buffer(), &mut self.delay)?;
        self.epd.sleep(&mut self.driver, &mut self.delay)?;
        Ok(())
    }

    pub fn render_controls_if_dirty(&mut self, background: OctColor, controls: &mut [&mut dyn Control]) -> anyhow::Result<()> {
        if !controls.iter().any(|c| c.is_dirty()) {
            return Ok(());
        }

        self.render(|d| {
            d.clear_buffer(background);

            for control in controls.iter() {
                control.render(d)?;
            }

            Ok(())
        })?;

        for control in controls.iter_mut() {
            control.clear_dirty();
        }
        
        Ok(())
    }

    pub fn width(&self) -> usize {
        self.display.width()
    }

    pub fn height(&self) -> usize {
        self.display.height()
    }

    pub fn bounding_box(&self) -> embedded_graphics::primitives::Rectangle {
        self.display.bounding_box()
    }
}

pub struct DisplayBuffer {
    buffer: &'static mut [u8; WIDTH as usize * HEIGHT as usize / 2],
    rotation: DisplayRotation,
}

impl DisplayBuffer {
    pub fn new() -> Self {
        Self {
            buffer: unsafe { &mut *addr_of_mut!(BUFFER) },
            rotation: DisplayRotation::default(),
        }
    }

    pub fn width(&self) -> usize {
        WIDTH as usize
    }

    pub fn height(&self) -> usize {
        HEIGHT as usize
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

impl Dimensions for DisplayBuffer {
    fn bounding_box(&self) -> embedded_graphics::primitives::Rectangle {
        embedded_graphics::primitives::Rectangle::new(Point::zero(), Size::new(WIDTH, HEIGHT))
    }
}

impl OctDisplay for DisplayBuffer {
    fn buffer(&self) -> &[u8] {
        self.buffer
    }
    
    fn get_mut_buffer(&mut self) -> &mut [u8] {
        self.buffer
    }
    
    fn set_rotation(&mut self, rotation: DisplayRotation) {
        self.rotation = rotation
    }
    
    fn rotation(&self) -> DisplayRotation {
        self.rotation
    }

    fn clear_buffer(&mut self, background_color: OctColor) {
        for byte in self.buffer.iter_mut() {
            *byte = OctColor::colors_byte(background_color, background_color);
        }
    }
}

impl DrawTarget for DisplayBuffer {
    type Color = OctColor;
    type Error = core::convert::Infallible;
    
    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>> {
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