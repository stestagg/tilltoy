use defmt::info;
use embassy_rp::{dma, interrupt::typelevel::Binding, pio::{self, InterruptHandler}, Peri};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

use crate::sk6812::{PioSk6812, PioSk6812Program};


pub use crate::sk6812::RGBW;

pub enum LedState {
    Color(RGBW),
    Default,
    Off,
    Noop,
}

// pub static LED_STATE: Signal<CriticalSectionRawMutex, LedState> = Signal::new();
pub static LED_STATE: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    LedState,
    1,
> = embassy_sync::channel::Channel::new();



pub struct Led<
    'a,
    PIO: pio::Instance,
    DMA: dma::Channel,
    DATA: pio::PioPin,
>
{
    pio_unit: Peri<'a, PIO>,
    dma: Peri<'a, DMA>,
    data_pin: Peri<'a, DATA>,
}

impl<
        'a,
        PIO: pio::Instance,
        DMA: dma::Channel,
        DATA: pio::PioPin,
    > Led<'a, PIO, DMA, DATA>
{
    pub fn new(
        pio_unit: Peri<'a, PIO>,
        dma: Peri<'a, DMA>,
        data_pin: Peri<'a, DATA>,
    ) -> Self {
        Self {
            pio_unit,
            dma,
            data_pin
        }
    }

    pub async fn run(self, irqs: impl Binding<PIO::Interrupt, InterruptHandler<PIO>>) -> () {
        let pio::Pio {
            mut common,
            sm1,
            .. // sm1, sm2, sm3
        } = pio::Pio::new(self.pio_unit, irqs);

        let program = PioSk6812Program::new(&mut common);
        let mut sk = PioSk6812::<PIO, 1, 1>::new(
            &mut common,
            sm1,
            self.dma,
            self.data_pin,
            &program,
        );
        info!("LED Configured");
        let startup: &[RGBW;1] = &[RGBW::new(0, 10, 0, 0)];
        sk.write(startup).await;
        info!("Led Initialized");

        loop {
            match LED_STATE.receive().await {
                LedState::Color(color) => {
                    let slice: &[RGBW;1] = &[color];
                    sk.write(slice).await;
                },
                LedState::Default => {
                    let slice: &[RGBW;1] = &[RGBW::new(0, 10, 0, 0)];
                    sk.write(slice).await;
                },
                LedState::Off => {
                    let slice: &[RGBW;1] = &[RGBW::black()];
                    sk.write(slice).await;
                },
                LedState::Noop => {
                    // Do nothing
                }
            }
        }
    }
}