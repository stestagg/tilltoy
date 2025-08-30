#![feature(impl_trait_in_assoc_type)]
#![no_std]
#![no_main]

pub mod led;
pub mod printer;
pub mod sk6812;
pub mod state;

use assign_resources::assign_resources;
use embassy_time::Timer;
use defmt::info;
use embassy_executor::task;
use embassy_time::Duration;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::Input;
use embassy_rp::peripherals;
use embassy_rp::peripherals::PIO1;
use embassy_rp::uart::Blocking;
use embassy_rp::uart::Parity;
use embassy_rp::uart::Uart;
use embassy_rp::uart::{Config, DataBits, StopBits};
use embassy_rp::Peri;
use embedded_io::Write;
use embassy_rp::pio::{InterruptHandler};
use crate::printer::Images;
use crate::state::InputEvent;
use crate::state::INPUT_EVENTS;

use {defmt_rtt as _, panic_probe as _};

assign_resources! {
    uart: UartResources {
        uart: UART1,
        tx_pin: PIN_4,
        rx_pin: PIN_5,
        rts_pin: PIN_7,
        cts_pin: PIN_6,
    },

    keys: KeyResources {
        key_1: PIN_20,
        key_2: PIN_19,
        key_3: PIN_18,
        key_4: PIN_17,
        key_5: PIN_11,
        key_6: PIN_12,
        key_7: PIN_13,
        key_8: PIN_14,
        total: PIN_15,
        void: PIN_16
    },

    led: LedResources {
        pio: PIO1,
        dma: DMA_CH4,
        data_pin: PIN_28,
    }

}

bind_interrupts!(struct Irqs {
    PIO1_IRQ_0 => InterruptHandler<PIO1>;
});

pub struct UartWrap<'a>(Uart<'a, Blocking>);

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

#[task]
async fn led_task(led: LedResources) {
    let runner = led::Led::new(led.pio, led.dma, led.data_pin);
    runner.run(Irqs).await;
}

#[task]
async fn printer_driver(printer: escpos_embedded::Printer<UartWrap<'static>>) {
    printer::driver(printer).await;
}

#[task(pool_size=8)]
async fn produce_button_task(mut btn: Input<'static>, image: Images, price: u16) {
    loop {
         btn.wait_for_any_edge().await;
        if btn.is_low() {
            info!("Button pressed: {:?}", image);
            INPUT_EVENTS.send(InputEvent::ProduceButtonPressed {
                image: image,
                price,
            }).await;
            Timer::after(Duration::from_millis(200)).await;
        }
        while btn.is_low() {
            Timer::after(Duration::from_millis(100)).await;
        }
    }
}

#[task]
async fn void_button_task(mut btn: Input<'static>) {
    loop {
        btn.wait_for_any_edge().await;
        if btn.is_low() {
            info!("Void button pressed");
            INPUT_EVENTS.send(InputEvent::VoidButtonPressed).await;
            Timer::after(Duration::from_millis(200)).await;
        }
        while btn.is_low() {
            Timer::after(Duration::from_millis(100)).await;
        }
    }
}

#[task]
async fn total_button_task(mut btn: Input<'static>) {
    loop {
        btn.wait_for_any_edge().await;
        if btn.is_low() {
            info!("Total button pressed");
            INPUT_EVENTS.send(InputEvent::TotalButtonPressed).await;
            Timer::after(Duration::from_millis(200)).await;
        }
        while btn.is_low() {
            Timer::after(Duration::from_millis(100)).await;
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let r = split_resources!(p);
    let uart_pins = r.uart;

    info!("Starting up...");

    let mut config = Config::default();
    config.baudrate = 115200;
    config.data_bits = DataBits::DataBits8;
    config.stop_bits = StopBits::STOP1;
    config.parity = Parity::ParityNone;

    let uart = Uart::new_with_rtscts_blocking(
        uart_pins.uart,
        uart_pins.tx_pin,
        uart_pins.rx_pin,
        uart_pins.rts_pin,
        uart_pins.cts_pin,
        config,
    );
    let printer = escpos_embedded::Printer::new(UartWrap(uart));
    spawner.spawn(printer_driver(printer)).unwrap();
    spawner.spawn(led_task(r.led)).unwrap();

    spawner.spawn(state::main_state()).unwrap();

    spawner.spawn(produce_button_task(
        Input::new(r.keys.key_1, embassy_rp::gpio::Pull::Up),
        Images::Garlic,
        1
    )).unwrap();
    spawner.spawn(produce_button_task(
        Input::new(r.keys.key_2, embassy_rp::gpio::Pull::Up),
        Images::Carrot,
        2
    )).unwrap();
    spawner.spawn(produce_button_task(
        Input::new(r.keys.key_3, embassy_rp::gpio::Pull::Up),
        Images::Corn,
        3
    )).unwrap() ;
    spawner.spawn(produce_button_task(
        Input::new(r.keys.key_4, embassy_rp::gpio::Pull::Up),
        Images::Tomato,
        4
    )).unwrap() ;
    spawner.spawn(produce_button_task(
        Input::new(r.keys.key_5, embassy_rp::gpio::Pull::Up),
        Images::Mushroom,
        5
    )).unwrap() ;
    spawner.spawn(produce_button_task(
        Input::new(r.keys.key_6, embassy_rp::gpio::Pull::Up),
        Images::Aubergine,
        6
    )).unwrap() ;
    spawner.spawn(produce_button_task(
        Input::new(r.keys.key_7, embassy_rp::gpio::Pull::Up),
        Images::Pumpkin,
        7
    )).unwrap() ;
    spawner.spawn(produce_button_task(
        Input::new(r.keys.key_8, embassy_rp::gpio::Pull::Up),
        Images::Croissant,
        8
    )).unwrap() ;
    spawner.spawn(void_button_task(
        Input::new(r.keys.void, embassy_rp::gpio::Pull::Up)
    )).unwrap();
    spawner.spawn(total_button_task(
        Input::new(r.keys.total, embassy_rp::gpio::Pull::Up)
    )).unwrap();

}
