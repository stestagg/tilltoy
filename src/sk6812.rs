use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::dma::{AnyChannel, Channel};
use embassy_rp::pio::program::pio_asm;
use embassy_rp::pio::{
    Common, Config, Direction, FifoJoin, Instance, LoadedProgram, PioPin, ShiftConfig, ShiftDirection, StateMachine
};
use embassy_rp::dma::Channel as Peripheral;
use embassy_rp::Peri;
use embassy_time::Timer;
use fixed::types::U24F8;


#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct RGBWParts {
    pub w: u8,
    pub b: u8,
    pub r: u8,
    pub g: u8,
}

#[derive(Clone, Copy)]
pub union RGBW {
    pub parts: RGBWParts,
    pub raw: [u8; 4],
    pub raw32: u32,
}

impl RGBW {
    pub const fn new(r: u8, g: u8, b: u8, w: u8) -> Self {
        Self {
            parts: RGBWParts { r, g, b, w },
        }
    }

    pub const fn black() -> Self {
        Self::new(0, 0, 0, 0)
    }

    pub const fn full_on() -> Self {
        Self::new(255, 255, 255, 255)
    }
}

const T1: u8 = 2; // start bit
const T2: u8 = 5; // data bit
const T3: u8 = 3; // stop bit
const CYCLES_PER_BIT: u32 = (T1 + T2 + T3) as u32;

/// This struct represents a ws2812 program loaded into pio instruction memory.
pub struct PioSk6812Program<'a, PIO: Instance> {
    prg: LoadedProgram<'a, PIO>,
}

impl<'a, PIO: Instance> PioSk6812Program<'a, PIO> {
    /// Load the Sk6812 program into the given pio
    pub fn new(common: &mut Common<'a, PIO>) -> Self {
        let prg = pio_asm!(r#"
        .side_set 1
        .define T1 2
        .define T2 5
        .define T3 3

        .wrap_target
        bitloop:
            out x, 1       side 0 [T3 - 1]
            jmp !x do_zero side 1 [T1 - 1]
            jmp bitloop    side 1 [T2 - 1]

        do_zero:
            nop            side 0 [T2 - 1]
        .wrap
        "#);                
        let prg = common.load_program(&prg.program);
        Self { prg }
    }
}

/// Pio backed sk6812 driver
/// Const N is the number of sk6812 leds attached to this pin
pub struct PioSk6812<'d, PIO: Instance, const S: usize, const N: usize> {
    dma: Peri<'d, AnyChannel>, // 
    sm: StateMachine<'d, PIO, S>,
}

impl<'d, P: Instance, const S: usize, const N: usize> PioSk6812<'d, P, S, N> {
    /// Configure a pio state machine to use the loaded Sk6812 program.
    pub fn new(
        pio: &mut Common<'d, P>,
        mut sm: StateMachine<'d, P, S>,
        dma: Peri<'d, impl Channel>,
        pin: Peri<'d, impl PioPin>,
        program: &PioSk6812Program<'d, P>,
    ) -> Self {
        // Setup sm0
        let mut cfg = Config::default();

        // Pin config
        let out_pin = pio.make_pio_pin(pin);
        cfg.set_out_pins(&[&out_pin]);
        cfg.set_set_pins(&[&out_pin]);

        cfg.use_program(&program.prg, &[&out_pin]);

        // Clock config, measured in kHz to avoid overflows
        let clock_freq = U24F8::from_num(clk_sys_freq() / 1000);
        let sk6812_freq = U24F8::from_num(800);
        let bit_freq = sk6812_freq * CYCLES_PER_BIT;
        cfg.clock_divider = clock_freq / bit_freq;

        // FIFO config
        cfg.fifo_join = FifoJoin::TxOnly;
        cfg.shift_out = ShiftConfig {
            auto_fill: true,
            threshold: 32,
            direction: ShiftDirection::Left,
        };

        sm.set_config(&cfg);
        sm.set_pin_dirs(Direction::Out, &[&out_pin]);

        sm.set_enable(true);

        Self {
            dma: dma.into(),
            sm,
        }
    }

    /// Write a buffer of RGBW to the ws2812 string
    pub async fn write(&mut self, colors: &[RGBW; N]) {
        // Precompute the word bytes from the colors
        let mut words = [0u32; N];
        for i in 0..N {
            words[i] = unsafe { colors[i].raw32 };
        }

        // DMA transfer
        self.sm.tx().dma_push(self.dma.reborrow(), &words, false).await;

        Timer::after_micros(55).await;
    }
}
