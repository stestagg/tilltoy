use embassy_executor::task;
use embassy_time::{Duration, Timer};
use escpos_embedded::Image;

use crate::{led::{Led, LedState, LED_STATE, RGBW}, printer::{DriverEvent, Images, PRINT_EVENTS}};

pub enum InputEvent {
    ProduceButtonPressed{ image: Images, price: u16 },
    VoidButtonPressed,
    TotalButtonPressed,
}

// Queue
pub static INPUT_EVENTS: embassy_sync::channel::Channel<
    embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
    InputEvent,
    1,
> = embassy_sync::channel::Channel::new();

async fn err_toggle() {
    for _ in 0..3 {
        LED_STATE.send(LedState::Color(RGBW::new(128, 0, 0, 0))).await;
        Timer::after(Duration::from_millis(200)).await;
        LED_STATE.send(LedState::Default).await;
        Timer::after(Duration::from_millis(200)).await;
    }
}

async fn set_led_state(color: Option<RGBW>) {
    let state = match color {
        Some(c) => LedState::Color(c),
        None => LedState::Default,
    };
    LED_STATE.send(state).await;
    LED_STATE.send(LedState::Noop).await;
}

#[task]
pub async fn main_state() {

    let mut in_transaction: bool = false;
    let mut current_price: u16 = 0;

    loop {
        match INPUT_EVENTS.receive().await {
            InputEvent::ProduceButtonPressed { image, price } => {
                set_led_state(Some(RGBW::new(0,0, 64, 0))).await;
                if ! in_transaction {
                    current_price = 0;
                    in_transaction = true;
                    PRINT_EVENTS.send(DriverEvent::PrintHeader).await;
                }

                if current_price + price as u16 > 999 {
                    err_toggle().await;
                } else {
                    current_price += price as u16;
                    PRINT_EVENTS.send(DriverEvent::PrintLine { image, price }).await;
                }
            }
            InputEvent::VoidButtonPressed => {
                set_led_state(Some(RGBW::new(0,0, 64, 0))).await;
                if in_transaction {
                    PRINT_EVENTS.send(DriverEvent::PrintVoid).await;
                    in_transaction = false;
                    current_price = 0;
                } else {
                    err_toggle().await;
                }
            }
            InputEvent::TotalButtonPressed => {
                set_led_state(Some(RGBW::new(0,0, 64, 0))).await;
                if in_transaction {
                    PRINT_EVENTS.send(DriverEvent::PrintTotal { price: current_price}).await;
                    in_transaction = false;
                    current_price = 0;
                } else {
                    err_toggle().await;
                }
            }
        }
        Timer::after(Duration::from_millis(400)).await;
        set_led_state(None).await;
    }

}