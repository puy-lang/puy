//! The isolate's clock layer (requirements §8.8; `docs/design/test-framework.md` 3.12,
//! `docs/spec/bytecode.md` 2.22).
//!
//! Every time a program reads (`ClockNow`, `ClockMono`) and every wait on time (`ClockSleep`)
//! goes through here. The layer is real (the host's `Clock`) until a test freezes it
//! (`ClockControl`); then it is virtual and holds both clocks in the isolate, without calling the
//! host. The time the runtime watches with (the time limit of `puy test`, …) does not come here,
//! so it stays real.

use puy_host::HostSet;

use super::host::host_rt_error;
use super::{Isolate, RtError, Step};
use crate::bytecode::{
    CLOCK_ADVANCE, CLOCK_FREEZE, CLOCK_FREEZE_AT, CLOCK_NOT_FROZEN, CLOCK_OK, CLOCK_OUT_OF_RANGE,
    CLOCK_SET_NOW,
};

/// Where the isolate's time comes from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ClockLayer {
    /// The host's `Clock`.
    #[default]
    Real,
    /// Stopped: the wall clock (UNIX nanoseconds) and the monotonic clock (nanoseconds) move only
    /// when the program moves them.
    Virtual { wall: i64, mono: u64 },
}

/// The host's wall clock in UNIX nanoseconds. Without a clock, `ENotSupportedException`.
fn host_wall(host: &mut HostSet) -> Result<i64, RtError> {
    let absent = host.absent.clock;
    match host.clock.as_mut() {
        Some(clock) => clock.wall_nanos().map_err(|e| host_rt_error(&e)),
        None => Err(host_rt_error(&absent.error("the clock"))),
    }
}

/// The host's monotonic clock in nanoseconds. Without a clock, `ENotSupportedException`.
fn host_mono(host: &mut HostSet) -> Result<u64, RtError> {
    let absent = host.absent.clock;
    match host.clock.as_mut() {
        Some(clock) => clock.monotonic_nanos().map_err(|e| host_rt_error(&e)),
        None => Err(host_rt_error(&absent.error("the clock"))),
    }
}

impl ClockLayer {
    /// The wall clock in UNIX nanoseconds (`ClockNow`, `I117`).
    pub(super) fn now(&self, host: &mut HostSet) -> Result<i64, RtError> {
        match *self {
            ClockLayer::Real => host_wall(host),
            ClockLayer::Virtual { wall, .. } => Ok(wall),
        }
    }

    /// The monotonic clock in nanoseconds (`ClockMono`, `I120`): the bits of the `u64` as a
    /// signed 64-bit value.
    pub(super) fn mono(&self, host: &mut HostSet) -> Result<i64, RtError> {
        match *self {
            ClockLayer::Real => host_mono(host).map(|n| n as i64),
            ClockLayer::Virtual { mono, .. } => Ok(mono as i64),
        }
    }

    /// Moves both virtual clocks `nanos` on (`Advance`, `Sleep`): `CLOCK_OUT_OF_RANGE` if either
    /// would go past its range (nothing moves then), `CLOCK_NOT_FROZEN` on the real clock.
    fn advance(&mut self, nanos: i64) -> i64 {
        let ClockLayer::Virtual { wall, mono } = self else {
            return CLOCK_NOT_FROZEN;
        };
        let moved = u64::try_from(nanos).ok().and_then(|n| {
            let w = wall.checked_add(i64::try_from(n).ok()?)?;
            let m = mono.checked_add(n)?;
            Some((w, m))
        });
        match moved {
            Some((w, m)) => {
                (*wall, *mono) = (w, m);
                CLOCK_OK
            }
            None => CLOCK_OUT_OF_RANGE,
        }
    }

    /// `ClockControl` (`I151`): what `kind` does with `value`, and its status.
    pub(super) fn control(
        &mut self,
        host: &mut HostSet,
        kind: i64,
        value: i64,
    ) -> Result<i64, RtError> {
        match kind {
            CLOCK_FREEZE => {
                // Stops at the real time; a frozen clock stays where it is
                if *self == ClockLayer::Real {
                    let wall = host_wall(host)?;
                    let mono = host_mono(host)?;
                    *self = ClockLayer::Virtual { wall, mono };
                }
                Ok(CLOCK_OK)
            }
            CLOCK_FREEZE_AT => {
                match self {
                    ClockLayer::Virtual { wall, .. } => *wall = value,
                    ClockLayer::Real => {
                        // The monotonic clock goes on from the real one, so that a stopwatch
                        // started before stays right; from 0 on a host without a clock
                        let mono = if host.clock.is_some() {
                            host_mono(host)?
                        } else {
                            0
                        };
                        *self = ClockLayer::Virtual { wall: value, mono };
                    }
                }
                Ok(CLOCK_OK)
            }
            CLOCK_ADVANCE => Ok(self.advance(value)),
            CLOCK_SET_NOW => match self {
                ClockLayer::Virtual { wall, .. } => {
                    *wall = value;
                    Ok(CLOCK_OK)
                }
                ClockLayer::Real => Ok(CLOCK_NOT_FROZEN),
            },
            _ => Ok(CLOCK_OUT_OF_RANGE),
        }
    }
}

impl Isolate {
    /// `ClockSleep` (`I152`): waits `nanos` nanoseconds. On the virtual clock it does not wait and
    /// moves both clocks on (the form for one task of "move on to the next timer when every task
    /// waits for time"; M5c widens it); `rA :=` its status. On the real clock, standard output
    /// is written out and the host's `Clock::sleep` waits; `rA := CLOCK_OK`. If writing out
    /// standard output waits, the instruction at `pc` runs again once it is written.
    pub(super) fn clock_sleep(&mut self, ra: usize, nanos: i64, pc: u32, next: u32) -> Step {
        if let ClockLayer::Virtual { .. } = self.clock {
            self.task.regs[ra] = self.clock.advance(nanos);
            self.set_pc(next);
            return Step::Continue;
        }
        let Ok(nanos) = u64::try_from(nanos) else {
            self.task.regs[ra] = CLOCK_OUT_OF_RANGE;
            self.set_pc(next);
            return Step::Continue;
        };
        match self.flush_out(pc) {
            Step::Continue => {}
            step => return step,
        }
        self.task.regs[ra] = CLOCK_OK;
        let absent = self.host.absent.clock;
        let Some(clock) = self.host.clock.as_mut() else {
            return Step::Error(host_rt_error(&absent.error("the clock")));
        };
        let fut = clock.sleep(nanos);
        self.await_host(fut, super::host::Deliver::Drop, next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A negative move is out of range and moves nothing. Pascal cannot reach it (`Advance`
    // checks the sign, `Sleep` takes a `Cardinal`), so only the VM's side is checked here.
    #[test]
    fn advance_negative_moves_nothing() {
        let mut c = ClockLayer::Virtual { wall: 5, mono: 7 };
        assert_eq!(c.advance(-1), CLOCK_OUT_OF_RANGE);
        assert_eq!(c, ClockLayer::Virtual { wall: 5, mono: 7 });
    }
}
