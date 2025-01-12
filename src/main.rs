#![no_std]
#![no_main]
extern crate heapless;
mod cycles_computer;
mod datetime_converter;
mod dcf77_decoder;
mod time_display;

use crate::stm32f4xx_hal::i2c::I2c;
use datetime_converter::DCF77DateTimeConverter;
use dcf77_decoder::DCF77Decoder;
use panic_rtt_target as _;

use chrono::naive::NaiveDateTime;
use cortex_m::peripheral::DWT;
use cycles_computer::CyclesComputer;
use feather_f405::hal as stm32f4xx_hal;
use feather_f405::{hal::prelude::*, pac, setup_clocks};
use ht16k33::{Dimming, Display, HT16K33};
use rtcc::Rtcc;
use rtic::app;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::gpio::{gpioa, gpiob, gpioc, AlternateOD, Input, Output, PullUp, PushPull, AF4};
use stm32f4xx_hal::rtc::Rtc;
use stm32f4xx_hal::timer::{CountDownTimer, Event, Timer};
use time_display::{display_error, show_rtc_time};

type SegmentDisplay =
    HT16K33<I2c<pac::I2C1, (gpiob::PB6<AlternateOD<AF4>>, gpiob::PB7<AlternateOD<AF4>>)>>;

fn sync_rtc(rtc: &mut Rtc, dt: &NaiveDateTime) {
    rtc.set_datetime(dt).expect("To be able to set datetime");
}

const DISP_I2C_ADDR: u8 = 0x70;
#[app(device = feather_f405::hal::stm32, monotonic = rtic::cyccnt::CYCCNT, 
      peripherals = true)]
const APP: () = {
    struct Resources {
        segment_display: SegmentDisplay,
        dcf_pin: gpioa::PAn<Input<PullUp>>,
        debug_pin: gpioc::PCn<Output<PushPull>>,
        timer: CountDownTimer<pac::TIM2>,
        cycles_computer: CyclesComputer,
        val: u16,
        decoder: DCF77Decoder,
        rtc: Rtc,
        synchronized: bool,
    }
    #[init(spawn=[])]
    fn init(cx: init::Context) -> init::LateResources {
        rtt_init_print!();
        let mut core = cx.core;
        let device = cx.device;
        core.DCB.enable_trace();
        DWT::unlock();
        core.DWT.enable_cycle_counter();
        //core.SCB.set_sleepdeep();

        let clocks = setup_clocks(device.RCC);
        let _syscfg = device.SYSCFG.constrain();
        let _exti = device.EXTI;
        let mut pwr = device.PWR;

        let gpiob = device.GPIOB.split();
        let scl = gpiob.pb6.into_alternate().set_open_drain();
        let sda = gpiob.pb7.into_alternate().set_open_drain();
        let i2c = I2c::new(device.I2C1, (scl, sda), 400.khz(), clocks);
        let mut ht16k33 = HT16K33::new(i2c, DISP_I2C_ADDR);
        ht16k33.initialize().expect("Failed to initialize ht16k33");
        ht16k33
            .set_display(Display::ON)
            .expect("Could not turn on the display!");
        ht16k33
            .set_dimming(Dimming::BRIGHTNESS_MAX)
            .expect("Could not set dimming!");
        display_error(&mut ht16k33, 0);
        ht16k33
            .write_display_buffer()
            .expect("Could not write 7-segment display");
        let gpioa = device.GPIOA.split();
        let pin = gpioa.pa6.into_pull_up_input().erase_number();

        // Use this pin for debugging decoded signal state with oscilloscope
        let gpioc = device.GPIOC.split();
        let output_pin = gpioc.pc6.into_push_pull_output().erase_number();
        // let pin = gpioa.pa6.into_floating_input().downgrade();
        //pa6.make_interrupt_source(&mut syscfg);
        //pa6.trigger_on_edge(&mut exti, Edge::RISING_FALLING);
        //pa6.enable_interrupt(&mut exti);

        let mut timer = Timer::new(device.TIM2, &clocks).start_count_down(100.hz());
        timer.listen(Event::TimeOut);
        let rtc = Rtc::new(device.RTC, 255, 127, false, &mut pwr);
        rprintln!("Init successful");
        init::LateResources {
            segment_display: ht16k33,
            dcf_pin: pin,
            debug_pin: output_pin,
            timer,
            cycles_computer: CyclesComputer::new(clocks.sysclk()),
            val: 0,
            decoder: DCF77Decoder::new(),
            rtc,
            synchronized: false,
        }
    }

    #[allow(clippy::empty_loop)]
    #[idle()]
    fn idle(_cx: idle::Context) -> ! {
        rprintln!("idle");
        loop {}
    }

    #[task(binds = TIM2, priority=2, resources=[timer, decoder, dcf_pin, 
           debug_pin, segment_display, rtc, synchronized])]
    fn tim2(cx: tim2::Context) {
        cx.resources.timer.clear_interrupt(Event::TimeOut);
        let pin_high = cx.resources.dcf_pin.is_high();
        let decoder = cx.resources.decoder;
        let debug_pin = cx.resources.debug_pin;
        decoder.read_bit(pin_high);

        match decoder.current_level() {
            true => drop(debug_pin.set_high()),
            false => drop(debug_pin.set_low()),
        };

        let mut v = 0;
        if let Some(datetime_bits) = decoder.last_bits() {
            decoder.reset_last_bits();
            let converter = DCF77DateTimeConverter::new(datetime_bits);
            match converter.dcf77_decoder() {
                Err(err) => {
                    rprintln!("Decoding error: {:?}", err);
                    v = 1;
                }
                Ok(dt) => {
                    rprintln!("Good date: {:?}", dt);
                    v = 8;
                    sync_rtc(cx.resources.rtc, &dt);
                    *cx.resources.synchronized = true;
                }
            }
        }
        let display = cx.resources.segment_display;
        show_rtc_time(cx.resources.rtc, display, *cx.resources.synchronized, v);
    }

    extern "C" {
        fn UART4();
    }
};
