#![feature(impl_trait_in_assoc_type)]
#![no_std]
#![no_main]

use assign_resources::assign_resources;
use cortex_m::asm::wfi;
use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals;
use embassy_rp::peripherals::UART1;
use embassy_rp::uart::Blocking;
use embassy_rp::uart::BufferedInterruptHandler;
use embassy_rp::uart::Parity;
use embassy_rp::uart::Uart;
use embassy_rp::uart::{BufferedUart, Config, DataBits, StopBits};
use embassy_rp::Peri;
use embassy_time::Duration;
use embassy_time::Timer;
use embedded_io::Read;
use embedded_io::Write;
use escpos_embed_image::{embed_image, embed_images};
use escpos_embedded::Delay;
use escpos_embedded::FromEmbeddedIo;
use escpos_embedded::Image;
use escpos_embedded::PrintSpeed;

use escpos_embedded::TimingModel;
// Ensure Image uses a mutable reference for data
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};


assign_resources! {
    uart: UartResources {
        uart: UART1,
        tx_pin: PIN_4,
        rx_pin: PIN_5,
        rts_pin: PIN_7,
        cts_pin: PIN_6,
    },

}

embed_images!(
    enum Images {
        #[pattern("gfx/*.png")]
    }
);

const fn image_from_char(c: char) -> &'static Image<&'static [u8]> {
    match c {
        ' ' => &Images::Space.get_image(),
        'x' => &Images::X.get_image(),
        '£' => &Images::Pound.get_image(),
        '1' => &Images::One.get_image(),
        '2' => &Images::Two.get_image(),
        '3' => &Images::Three.get_image(),
        '4' => &Images::Four.get_image(),
        '5' => &Images::Five.get_image(),
        '6' => &Images::Six.get_image(),
        '7' => &Images::Seven.get_image(),
        '8' => &Images::Eight.get_image(),
        '9' => &Images::Nine.get_image(),
        '0' => &Images::Zero.get_image(),
        _ => core::panic!("Unsupported character"), // Default to logo for unsupported characters
    }
}


const FRAMEBUFFER_SIZE: usize = (384 / 8) * 80; // 384 pixels wide, 80 pixels tall, 1 bit per pixel

trait Framebuffer {
    fn clear(&mut self);
    fn blit_image<U: AsRef<[u8]>>(&mut self, src: &Image<U>, x_offset: u16, y_offset: u16);
}

impl<const N: usize> Framebuffer for Image<[u8; N]> {
    fn clear(&mut self) {
        self.data.fill(0);
    }

    fn blit_image<U: AsRef<[u8]>>(&mut self, src: &Image<U>, x_offset: u16, y_offset: u16) {
        let dest_stride = (self.width + 7) / 8;
        let src_stride = (src.width + 7) / 8;
        let src_data = src.data.as_ref();

        for y in 0..src.height {
            let dy = y + y_offset;
            if dy >= self.height {
                continue;
            }

            for x in 0..src.width {
                let dx = x + x_offset;
                if dx >= self.width {
                    continue;
                }

                let src_byte = src_data[(y as usize) * (src_stride as usize) + (x / 8) as usize];
                let src_bit = 7 - (x % 8);
                let dest_idx = (dy as usize) * (dest_stride as usize) + (dx / 8) as usize;
                let dest_bit = 7 - (dx % 8);
                if ((src_byte >> src_bit) & 1) != 0 {
                    self.data[dest_idx] |= 1 << dest_bit;
                } else {
                    self.data[dest_idx] &= !(1 << dest_bit);
                }
            }
        }
    }

}

// fn print_line_item(printer: &mut escpos_embedded::Printer, item: Images, quantity: u8, price: u8) {
//     let mut line = [' '; 128];
//     core::write!(line, "x{quantity}  £{price}").unwrap();
//     let chars = line.chars().rev();
//     let mut cur_pos: i16 = 384 - 10;
//     for c in chars.iter().rev() {
//         let img = image_from_char(*c);
//         cur_pos -= img.width as i16;
//         if cur_pos < 0 {
//             break;
//         }
//         fb_image.blit_image(img, cur_pos as u16, 0);
//     }
    
// }

struct UartWrap<'a>(Uart<'a, Blocking>);

impl<'a> escpos_embedded::Write for UartWrap<'a> {
    type Error = embassy_rp::uart::Error;

    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        self.0.blocking_write(buf)?;
        self.0.flush()
    }

}
impl<'a> escpos_embedded::Read for UartWrap<'a> {
    type Error = embassy_rp::uart::Error;

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
            self.0.blocking_read(buf)?;
            Ok(buf.len())
        }
}

struct PrinterDelay{}
impl Delay for PrinterDelay {
    fn delay_ms(&mut self, ms: u32) {
        embassy_time::block_for(Duration::from_millis(ms as u64));
    }
}
 
#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let r = split_resources!(p);
    let uart_pins = r.uart;

    let mut config = Config::default();
    config.baudrate = 115200;
    config.data_bits = DataBits::DataBits8;
    config.stop_bits = StopBits::STOP1; 
    config.parity = Parity::ParityNone;
    // config.invert_cts = true;
    // config.invert_rts = true;

    let mut uart = Uart::new_with_rtscts_blocking(
        uart_pins.uart,
        uart_pins.tx_pin,
        uart_pins.rx_pin,
        uart_pins.rts_pin,
        uart_pins.cts_pin,
        config,
    );
    let mut printer = escpos_embedded::Printer::new(UartWrap(uart));

    let timing_model = TimingModel::new(0, 0);
    let mut delay = PrinterDelay {};

    // printer.set_baud_rate(115200).unwrap();
    
    printer.set_software_flow_control(false).unwrap();
    printer.set_max_speed(200).unwrap();
    printer.set_print_speed(PrintSpeed::Speed3).unwrap();

    // sleep for a bit to allow the printer to initialize
    Timer::after(Duration::from_millis(2000)).await;

    printer.print_image_with_delay(
        &Images::Header.get_image(),
        &timing_model,
        &mut delay,
    ).unwrap();
    
    let _ = printer.paper_status().unwrap();
    // Timer::after(Duration::from_millis(5000)).await;

    printer.feed(2).unwrap();
    printer.raw(&[0x1B, 0x40]).unwrap();

    let mut fb_image: Image<[u8; FRAMEBUFFER_SIZE]> = Image {
        width: 384,
        height: 80,
        data: [0u8; FRAMEBUFFER_SIZE], // Ensure this is a mutable reference
    };
    printer.feed(2).unwrap();

    fb_image.clear();
    fb_image.blit_image(&Images::Banana.get_image(), 0, 0);
    let chars = ['x', '1', '0', ' ', ' ', '£', '1', '0'];
    let mut cur_pos: i16 = fb_image.width as i16 - 10;

    for c in chars.iter().rev() {
        let img = image_from_char(*c);
        cur_pos -= img.width as i16;
        if cur_pos < 0 {
            break;
        }
        fb_image.blit_image(img, cur_pos as u16, 0);
    }
    

    printer.print_image_with_delay(
        &fb_image,
        &timing_model,
        &mut delay,
    ).unwrap();
    printer.feed(2).unwrap();

    fb_image.clear();

    fb_image.blit_image(&Images::Juice.get_image(), 0, 0);
    let chars = ['x', '9', ' ', ' ', '£', '6'];
    let mut cur_pos: i16 = fb_image.width as i16 - 10;

    for c in chars.iter().rev() {
        let img = image_from_char(*c);
        cur_pos -= img.width as i16;
        if cur_pos < 0 {
            break;
        }
        fb_image.blit_image(img, cur_pos as u16, 0);
    }

    printer.print_image_with_delay(
        &fb_image,
        &timing_model,
        &mut delay,
    ).unwrap();

    printer.feed(2).unwrap();
    
    printer.print_image_with_delay(
        &Images::Footer.get_image(),
        &timing_model,
        &mut delay,
    ).unwrap();
    let _ = printer.paper_status().unwrap();
    // Timer::after(Duration::from_millis(5000)).await;
    printer.feed(2).unwrap();

    loop {
        wfi();
    }
}