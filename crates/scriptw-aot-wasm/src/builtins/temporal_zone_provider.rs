//! Private emitted Temporal proofs and the pinned named-zone data boundary.
//! Handles retain rooted GC values and Function-owned numeric coordinates.

use super::temporal_options::{Disambiguation, OffsetOption, StringValuedOption, TemporalOverflow};
use crate::gc_types::*;
use crate::{emit::FunctionBuilder, *};

mod exact;
mod identity;
mod policy;
mod provider_wire;
mod relative;
mod responses;

pub(super) use exact::*;
pub(super) use policy::TemporalZonedParseFlags;
pub(super) use relative::*;

pub(super) use scriptw_intl::SystemTimeZoneKind as TemporalZoneKind;
#[derive(Clone, Copy)]
pub(super) enum TemporalOffsetBehavior {
    Wall,
    Exact,
    Option,
}
#[derive(Clone, Copy)]
pub(super) enum TemporalOffsetMatchBehavior {
    MatchExactly,
    MatchMinutes,
}

/// Borrowed completed inputs for the sole GC ZonedDateTime allocator.
pub(super) struct TemporalZonedAllocationInput<'a> {
    instant: &'a NormalizedTemporalInstantLocals,
    zone: &'a ResolvedTemporalZoneLocals,
    calendar: &'a TemporalCalendarSlotLocals,
}
impl<'a> TemporalZonedAllocationInput<'a> {
    pub(super) const fn new(
        instant: &'a NormalizedTemporalInstantLocals,
        zone: &'a ResolvedTemporalZoneLocals,
        calendar: &'a TemporalCalendarSlotLocals,
    ) -> Self {
        Self {
            instant,
            zone,
            calendar,
        }
    }
    pub(super) fn instant(&self) -> &'a NormalizedTemporalInstantLocals {
        self.instant
    }
    pub(super) fn zone(&self) -> &'a ResolvedTemporalZoneLocals {
        self.zone
    }
    pub(super) fn calendar(&self) -> &'a TemporalCalendarSlotLocals {
        self.calendar
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_temporal_provider_corruption_if_i32(&self, function: &mut Function) {
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
    }
}
