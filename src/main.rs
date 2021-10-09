#![deny(unsafe_code)]
//#![deny(warnings)]
#![no_std]
#![no_main]

extern crate heapless;
mod cycles_computer;
mod datetime_converter;
mod dcf77_decoder;
mod second_sync;
mod time_display;

use rtic::cyccnt::Instant;

use crate::stm32f4xx_hal::i2c::I2c;
// use datetime_converter::DCF77DateTimeConverter;
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
use stm32f4xx_hal::gpio::{
    gpioa, gpiob, gpioc, AlternateOD, Edge, Input, Output, PullUp, PushPull, AF4,
};
use stm32f4xx_hal::rtc::Rtc;
//use stm32f4xx_hal::timer::{CountDownTimer, Event, Timer};
use time_display::display_error;

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
        cycles_computer: CyclesComputer,
        val: u16,
        decoder: DCF77Decoder,
        rtc: Rtc,
        synchronized: bool,
    }
    #[init(schedule = [dummy_schedule], spawn = [])]
    fn init(mut cx: init::Context) -> init::LateResources {
        rtt_init_print!();
        let mut core = cx.core;
        let device = cx.device;

        // Initialize (enable) the monotonic timer (CYCCNT)
        core.DCB.enable_trace();
        // required on Cortex-M7 devices that software lock the DWT (e.g. STM32F7)
        DWT::unlock();
        core.DWT.enable_cycle_counter();

        // semantically, the monotonic timer is frozen at time "zero" during `init`
        // NOTE do *not* call `Instant::now` in this context; it will return a nonsense value
        let now = cx.start; // the start time of the system

        let clocks = setup_clocks(device.RCC);
        let mut syscfg = device.SYSCFG.constrain();
        let mut exti = device.EXTI;
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

        // Configure input pin for DCF77 signal as interrup source
        let gpioa = device.GPIOA.split();
        let mut pin = gpioa.pa6.into_pull_up_input().erase_number();
        pin.make_interrupt_source(&mut syscfg);
        pin.trigger_on_edge(&mut exti, Edge::RisingFalling);
        pin.enable_interrupt(&mut exti);

        // Use this pin for debugging decoded signal state with oscilloscope
        let gpioc = device.GPIOC.split();
        let output_pin = gpioc.pc6.into_push_pull_output().erase_number();
        // let pin = gpioa.pa6.into_floating_input().downgrade();
        //pa6.make_interrupt_source(&mut syscfg);
        //pa6.trigger_on_edge(&mut exti, Edge::RISING_FALLING);
        //pa6.enable_interrupt(&mut exti);

        // Schedule the blinking task
        // cx.schedule.blink(cx.start + CYCLE_HZ.cycles()).unwrap();

        //let mut timer = Timer::new(device.TIM2, &clocks).start_count_down(100.hz());
        //timer.listen(Event::TimeOut);
        let rtc = Rtc::new(device.RTC, 255, 127, false, &mut pwr);
        let cc = CyclesComputer::new(clocks.sysclk());
        rprintln!("Init successful");
        init::LateResources {
            segment_display: ht16k33,
            dcf_pin: pin,
            debug_pin: output_pin,
            cycles_computer: CyclesComputer::new(clocks.sysclk()),
            val: 0,
            decoder: DCF77Decoder::new(cc),
            rtc,
            synchronized: false,
        }
    }

    #[task(binds = EXTI9_5, priority=2, resources=[dcf_pin, debug_pin,  cycles_computer, decoder])]
    fn exti9_5(cx: exti9_5::Context) {
        let dcf_pin = cx.resources.dcf_pin;
        let debug_pin = cx.resources.debug_pin;
        let dcf_interrupted = dcf_pin.check_interrupt();
        dcf_pin.clear_interrupt_pending_bit();
        if !dcf_interrupted {
            return;
        }
        let now = Instant::now();
        let res = cx
            .resources
            .decoder
            .register_transition(dcf_pin.is_high(), now, debug_pin);
        if let Err(e) = res {
            rprintln!("Err: {:?}", e);
        }
    }

    // NOTE This is a dummy task which is not called. It is required as a schedule target
    // to allow to start the time of the system
    #[task( schedule = [dummy_schedule])]
    fn dummy_schedule(_cx: dummy_schedule::Context) {
        // cx.resources.led.toggle().unwrap();
        // cx.schedule.blink(cx.scheduled + CYCLE_HZ.cycles()).unwrap();
    }

    #[allow(clippy::empty_loop)]
    #[idle()]
    fn idle(_cx: idle::Context) -> ! {
        rprintln!("idle");
        loop {}
    }

    extern "C" {
        fn UART4();
    }
};
