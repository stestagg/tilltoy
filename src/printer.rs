use crate::UartWrap;
use defmt::Format;
use escpos_embed_image::embed_images;
use escpos_embedded::{PrintSpeed, Printer};
use embassy_time::Duration;
use embassy_time::Timer;
use escpos_embedded::Image;
use escpos_embed_image::embed_image;

const FB_HEIGHT: usize = 238;

const FRAMEBUFFER_SIZE: usize = (384 / 8) * FB_HEIGHT; // 384 pixels wide, FB_HEIGHT pixels tall, 1 bit per pixel

embed_images!(
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Format)]
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

fn images_from_price(price: u16) -> [&'static Image<&'static [u8]>; 5] {
    let mut images = [Images::Space.get_image(); 5];
    let mut remaining = price;

    for i in 0..5 {
        if remaining == 0 && i < 5 {
            images[i] = Images::Pound.get_image();
            break;
        } else {
            let digit = remaining % 10;
            images[i] = image_from_char(char::from_digit(digit as u32, 10).unwrap());
            remaining /= 10;
        }
    }

    images
}


trait Framebuffer {   
    fn clear(&mut self);
    fn blit_image<U: AsRef<[u8]>>(&mut self, src: &Image<U>, x_offset: u16, y_offset: u16);

    fn head(&self, rows: u16) -> Image<&[u8]>;
}

impl<const N: usize> Framebuffer for Image<[u8; N]> {
    fn clear(&mut self) {
        self.data.fill(0);
    }

    fn head(&self, rows: u16) -> Image<&[u8]> {
        Image {
            width: self.width,
            height: rows,
            data: &self.data[..(rows as usize * self.width as usize / 8)],
        }
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


// Events
pub enum DriverEvent {
    PrintHeader,
    PrintLine { image: Images, price: u16 },
    PrintTotal { price: u16 },
    PrintVoid,
}
// Queue
pub static PRINT_EVENTS: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    DriverEvent,
    1,
> = embassy_sync::channel::Channel::new();


pub async fn driver(mut printer: Printer<UartWrap<'static>>) {

    let mut fb_image: Image<[u8; FRAMEBUFFER_SIZE]> = Image {
        width: 384,
        height: FB_HEIGHT as u16,
        data: [0u8; FRAMEBUFFER_SIZE],
    };

    printer.set_software_flow_control(false).unwrap();
    printer.set_max_speed(200).unwrap();
    printer.set_print_speed(PrintSpeed::Speed3).unwrap();

    Timer::after(Duration::from_millis(200)).await;


    // Main loop here
    loop {
        match PRINT_EVENTS.receive().await {            
            DriverEvent::PrintHeader => {
                printer.print_image(&Images::Header.get_image()).unwrap();
                printer.raw(&[0x0A]).unwrap();
            }
            DriverEvent::PrintLine { image, price } => {
                let produce_image = image.get_image();
                fb_image.clear();
                fb_image.blit_image(produce_image, 0, 0);

                let mut cur_x = fb_image.width as u16 - 10;
                let price_images = images_from_price(price);
                for img in price_images.iter() {
                    fb_image.blit_image(img, cur_x - img.width, 0);
                    cur_x -= img.width + 5;
                }
                printer.print_image(&fb_image.head(80)).unwrap();
            }
            DriverEvent::PrintTotal { price } => {
                printer.feed(1).unwrap();
                fb_image.clear();
                fb_image.blit_image(&Images::Footer.get_image(), 0, 0);

                let mut cur_x = fb_image.width - 20;
                let price_images = images_from_price(price);
                for img in price_images.iter() {
                    fb_image.blit_image(img, cur_x - img.width, 90);
                    cur_x -= img.width + 5;
                }

                printer.print_image(&fb_image).unwrap();
                printer.raw(&[0x0A, 0x0A, 0x0A]).unwrap();
            }
            DriverEvent::PrintVoid => {
                printer.raw(&[0x0A]).unwrap();
                printer.print_image(&Images::Void.get_image()).unwrap();
                printer.raw(&[0x0A, 0x0A, 0x0A]).unwrap();
            }
        }
    }

    // printer.print_image(&Images::Header.get_image()).unwrap();

    // let _ = printer.paper_status().unwrap();

    // printer.feed(2).unwrap();


    // printer.feed(2).unwrap();

    // fb_image.clear();
    // fb_image.blit_image(&Images::Banana.get_image(), 0, 0);
    // let chars = ['x', '1', '0', ' ', ' ', '£', '1', '0'];
    // let mut cur_pos: i16 = fb_image.width as i16 - 10;

    // for c in chars.iter().rev() {
    //     let img = image_from_char(*c);
    //     cur_pos -= img.width as i16;
    //     if cur_pos < 0 {
    //         break;
    //     }
    //     fb_image.blit_image(img, cur_pos as u16, 0);
    // }

    // printer.print_image(&fb_image).unwrap();
    // printer.feed(2).unwrap();

    // fb_image.clear();

    // fb_image.blit_image(&Images::Juice.get_image(), 0, 0);
    // let chars = ['x', '9', ' ', ' ', '£', '6'];
    // let mut cur_pos: i16 = fb_image.width as i16 - 10;

    // for c in chars.iter().rev() {
    //     let img = image_from_char(*c);
    //     cur_pos -= img.width as i16;
    //     if cur_pos < 0 {
    //         break;
    //     }
    //     fb_image.blit_image(img, cur_pos as u16, 0);
    // }

    // printer.print_image(&fb_image).unwrap();

    // printer.feed(2).unwrap();

    // printer.print_image(&Images::Footer.get_image()).unwrap();
    // let _ = printer.paper_status().unwrap();
    // // Timer::after(Duration::from_millis(5000)).await;
    // printer.feed(2).unwrap();
}
