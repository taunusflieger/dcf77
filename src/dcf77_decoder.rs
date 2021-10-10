use crate::cycles_computer::CyclesComputer;
use crate::second_sync::SecondSync;
use crate::stm32f4xx_hal::gpio::{gpioc, Edge, Output, PushPull};
use core::iter::IntoIterator;
use core::num::Wrapping;
use rtic::cyccnt::{Instant, U32Ext};
use rtt_target::rprintln;

#[derive(Debug)]
pub enum DecoderError {
    WrongTransition,
}

pub struct SignalSmoother<const X: usize> {
    buf: [bool; X],
    last_val: bool,
}

impl<const X: usize> SignalSmoother<X> {
    pub fn new() -> Self {
        Self {
            buf: [true; X],
            last_val: true,
        }
    }
    pub fn add_signal(&mut self, sig: bool) -> bool {
        self.buf.rotate_left(1);
        self.buf[X - 1] = sig;
        if self.buf.iter().all(|x| *x != self.last_val) {
            self.last_val = !self.last_val;
        }
        self.last_val
    }
}

pub struct DCF77Decoder {
    last_high_to_low: Option<Instant>,
    last_low_to_high: Option<Instant>,
    second_sync: SecondSync,
}

impl DCF77Decoder {
    pub fn new(cycles_computer: CyclesComputer) -> Self {
        Self {
            last_high_to_low: None,
            last_low_to_high: None,
            second_sync: SecondSync::new(cycles_computer),
        }
    }

    pub fn register_transition(
        &mut self,
        low_to_high: bool,
        now: Instant,
        debug_pin: &mut gpioc::PCn<Output<PushPull>>,
    ) -> Result<(), DecoderError> {
        let now = Instant::now();
        if low_to_high {
            debug_pin.set_high();
            self.last_low_to_high.replace(now);
            match self.second_sync.register_transition(Edge::Rising, now) {
                Ok(_) => (),
                Err(_e) => return Err(DecoderError::WrongTransition),
            }
        } else {
            debug_pin.set_low();
            self.last_high_to_low.replace(now);
            match self.second_sync.register_transition(Edge::Falling, now) {
                Ok(_) => (),
                Err(_e) => return Err(DecoderError::WrongTransition),
            }
        }

        Ok(())
    }
}
