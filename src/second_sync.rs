// Second Sync
// Let's say we want the system to be able to sync
// the second up to a signal/noise ratio of 1:99. This means we need to be able to
// handle 100 falling flanks per second. If we store the timestamp of the falling
// flanks in a list for 3 seconds, we need a list with 300 entries. After three
// seconds we look if we have at least two entries which have a timestamp
// difference of 1000ms => these should than be considered as candidates for a
// sync. We validate these candidates for another 3 seconds. If they are good, we
// have a sync of the second signal in under 10 seconds. If not, we start all
// over.
//
//
use crate::cycles_computer::CyclesComputer;
use crate::stm32f4xx_hal::gpio::Edge;
use core::iter::IntoIterator;
use core::num::Wrapping;
use rtic::cyccnt::{Instant, U32Ext, CYCCNT};
use rtic::Monotonic;
use rtt_target::rprintln;

#[derive(Debug)]
pub enum SecondSyncError {
    WrongTransition,
}

pub struct SecondSync {
    timestamps_edge_down: [u32; 300],
    timestamps_edge_up: [u32; 300],
    edge_down_idx: usize,
    edge_up_idx: usize,
}

impl SecondSync {
    pub fn new() -> Self {
        SecondSync {
            timestamps_edge_down: [0; 300],
            timestamps_edge_up: [0; 300],
            edge_down_idx: 0,
            edge_up_idx: 0,
        }
    }

    pub fn register_transition(
        &mut self,
        signal: Edge,
        now: Instant,
        cycles_computer: CyclesComputer,
    ) -> Result<(), SecondSyncError> {
        match signal {
            Edge::Falling => {
                self.timestamps_edge_down[self.edge_down_idx] = cycles_computer
                    .from_cycles(now.duration_since(CYCCNT::zero()))
                    .as_millis()
                    as u32;
            }
            Edge::Rising => {}
            _ => return Err(SecondSyncError::WrongTransition),
        }
        Ok(())
    }
    /*
        pub fn start_1ms_timer(tim1: pac::TIM1, clocks: &Clocks) -> pac::TIM1 {
            // pause
            tim1.cr1.modify(|_, w| w.cen().clear_bit());
            // reset counter
            tim1.cnt.reset();

            let ticks = clocks.pclk2().0; // for 1.hz() = 84 * 1E+6

            // let arr = u16(ticks / u32(psc + 1)).unwrap();
            let arr: u32 = 999; // 1000 bins

            let arr = arr << ARR_MULTIPL; // we can't fit more into psc
            let psc = u16((ticks / arr) - 1).unwrap(); // 42000
            tim1.psc.write(|w| w.psc().bits(psc));

            tim1.arr.write(|w| unsafe { w.bits(arr) });

            // Trigger update event to load the registers
            tim1.cr1.modify(|_, w| w.urs().set_bit());
            tim1.egr.write(|w| w.ug().set_bit());
            tim1.cr1.modify(|_, w| w.urs().clear_bit());

            // start counter
            tim1.cr1.modify(|_, w| w.cen().set_bit());
        }
    */
}
