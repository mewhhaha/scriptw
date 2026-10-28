//! One completed Chinese/Dangi year for the real calendar consumers.

mod month_day;
mod proleptic;

use super::*;
use crate::data::{
    TemporalEastAsianCalendar, TemporalEastAsianYearRowSlot, EAST_ASIAN_LEAP_ORDINAL_SHIFT,
    EAST_ASIAN_MONTH_MASK, EAST_ASIAN_NEW_YEAR_OFFSET_SHIFT, EAST_ASIAN_YEAR_ROW_BYTES,
    TEMPORAL_EAST_ASIAN_YEAR_ENVELOPE,
};
use crate::gc_types::I64Local;
use crate::runtime_helpers::{
    EastAsianYearCallResult, TemporalChineseYearArguments, TemporalChineseYearParameters,
    TemporalDangiYearArguments, TemporalDangiYearParameters,
};

/// Only the bounded central factory constructs this non-Copy year. Month
/// operations borrow the selected kind, year and all its metadata together.
pub(super) struct EastAsianYearLocals {
    kind: TemporalEastAsianCalendar,
    year: I64Local,
    start: I64Local,
    next_start: I64Local,
    month_mask: I64Local,
    leap_ordinal: I64Local,
    month_count: I64Local,
    first_month_serial: I64Local,
}

impl EastAsianYearLocals {
    fn reserve(
        builder: &mut FunctionBuilder<'_>,
        kind: TemporalEastAsianCalendar,
        function: &mut Function,
    ) -> Self {
        Self {
            kind,
            year: builder.runtime_schema().reserve_i64_local(function),
            start: builder.runtime_schema().reserve_i64_local(function),
            next_start: builder.runtime_schema().reserve_i64_local(function),
            month_mask: builder.runtime_schema().reserve_i64_local(function),
            leap_ordinal: builder.runtime_schema().reserve_i64_local(function),
            month_count: builder.runtime_schema().reserve_i64_local(function),
            first_month_serial: builder.runtime_schema().reserve_i64_local(function),
        }
    }

    fn result_locals(&self) -> [I64Local; 7] {
        [
            self.year,
            self.start,
            self.next_start,
            self.month_mask,
            self.leap_ordinal,
            self.month_count,
            self.first_month_serial,
        ]
    }

    pub(super) fn release(self, builder: &mut FunctionBuilder<'_>, function: &mut Function) {
        for local in self.result_locals().into_iter().rev() {
            builder.runtime_schema().release_i64_local(local, function);
        }
    }
}

impl EastAsianYearCallResult {
    fn store(self, model: &EastAsianYearLocals, function: &mut Function) {
        for local in model.result_locals().into_iter().rev() {
            local.store(function);
        }
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_temporal_east_asian_year_helper(
        &mut self,
        kind: TemporalEastAsianCalendar,
        real: bool,
    ) -> Function {
        let helper = RuntimeHelperId::for_east_asian_year(kind);
        if !real {
            return self.temporal_calendar_helper_stub(helper);
        }
        let mut function = self.begin_helper_body(helper);
        let year = match kind {
            TemporalEastAsianCalendar::Chinese => {
                self.helper_parameters::<TemporalChineseYearParameters>(&mut function)
                    .year
            }
            TemporalEastAsianCalendar::Dangi => {
                self.helper_parameters::<TemporalDangiYearParameters>(&mut function)
                    .year
            }
        };
        let model = EastAsianYearLocals::reserve(self, kind, &mut function);
        self.emit_temporal_east_asian_select_year_body(&model, year, &mut function);
        for local in model.result_locals() {
            local.load(&mut function);
        }
        model.release(self, &mut function);
        function.instruction(&Instruction::End);
        self.finish_function(function)
    }

    pub(super) fn emit_temporal_east_asian_year_model(
        &mut self,
        kind: TemporalEastAsianCalendar,
        year: I64Local,
        function: &mut Function,
    ) -> EastAsianYearLocals {
        let model = EastAsianYearLocals::reserve(self, kind, function);
        self.emit_temporal_east_asian_select_year(&model, year, function);
        model
    }

    fn emit_temporal_east_asian_select_year(
        &mut self,
        model: &EastAsianYearLocals,
        input: I64Local,
        function: &mut Function,
    ) {
        let base = self
            .runtime_helper_base()
            .expect("Temporal calendar arithmetic has a registered helper plan");
        let result = match model.kind {
            TemporalEastAsianCalendar::Chinese => self.runtime_schema().call_helper(
                TemporalChineseYearArguments::new(input),
                base,
                function,
            ),
            TemporalEastAsianCalendar::Dangi => self.runtime_schema().call_helper(
                TemporalDangiYearArguments::new(input),
                base,
                function,
            ),
        };
        result.store(model, function);
    }

    /// Field callers reject outside this broad envelope before month info.
    /// Canonical duration/rounding origins fit it. Clamp only the private
    /// working local so a future wrong internal input cannot overflow a model.
    fn emit_temporal_east_asian_select_year_body(
        &mut self,
        model: &EastAsianYearLocals,
        input: I64Local,
        function: &mut Function,
    ) {
        let image = self
            .strings
            .temporal_east_asian_year_image()
            .calendar(model.kind);
        let first = image.first_related_year();
        let count = i64::from(image.year_count());
        (input).load(function);
        (model.year).store(function);
        for (bound, comparison) in [
            (-TEMPORAL_EAST_ASIAN_YEAR_ENVELOPE, Instruction::I64LtS),
            (TEMPORAL_EAST_ASIAN_YEAR_ENVELOPE, Instruction::I64GtS),
        ] {
            (model.year).load(function);
            function.instruction(&Instruction::I64Const(bound));
            function.instruction(&comparison);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(bound));
            (model.year).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        (model.year).load(function);
        function.instruction(&Instruction::I64Const(first));
        function.instruction(&Instruction::I64GeS);
        (model.year).load(function);
        function.instruction(&Instruction::I64Const(first + count));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let pointer = self.strings.temporal_east_asian_year_image().install_rows(
            model.kind,
            self.runtime_schema(),
            function,
        );
        let address = self.runtime_schema().reserve_i64_local(function);
        let packed = self.runtime_schema().reserve_i64_local(function);
        let one = self.runtime_schema().reserve_i64_local(function);
        (model.year).load(function);
        function.instruction(&Instruction::I64Const(first));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(EAST_ASIAN_YEAR_ROW_BYTES as i64));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Const(i64::from(pointer)));
        function.instruction(&Instruction::I64Add);
        (address).store(function);
        (address).load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load32U(Self::memarg32(
            TemporalEastAsianYearRowSlot::Packed.byte_offset(),
        )));
        (packed).store(function);
        (address).load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load32S(Self::memarg32(
            TemporalEastAsianYearRowSlot::MonthPrefix.byte_offset(),
        )));
        (model.first_month_serial).store(function);
        (packed).load(function);
        function.instruction(&Instruction::I64Const(i64::from(EAST_ASIAN_MONTH_MASK)));
        function.instruction(&Instruction::I64And);
        (model.month_mask).store(function);
        (packed).load(function);
        function.instruction(&Instruction::I64Const(i64::from(
            EAST_ASIAN_LEAP_ORDINAL_SHIFT,
        )));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(15));
        function.instruction(&Instruction::I64And);
        (model.leap_ordinal).store(function);
        (model.leap_ordinal).load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(12));
        function.instruction(&Instruction::I64Add);
        (model.month_count).store(function);
        function.instruction(&Instruction::I64Const(1));
        (one).store(function);
        self.emit_temporal_plain_date_epoch_days(model.year, one, one, model.start, function);
        (model.start).load(function);
        (packed).load(function);
        function.instruction(&Instruction::I64Const(i64::from(
            EAST_ASIAN_NEW_YEAR_OFFSET_SHIFT,
        )));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Add);
        (model.start).store(function);
        (model.start).load(function);
        (model.month_count).load(function);
        function.instruction(&Instruction::I64Const(29));
        function.instruction(&Instruction::I64Mul);
        (model.month_mask).load(function);
        function.instruction(&Instruction::I64Popcnt);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Add);
        (model.next_start).store(function);
        self.runtime_schema().release_i64_local(one, function);
        self.runtime_schema().release_i64_local(packed, function);
        self.runtime_schema().release_i64_local(address, function);
        function.instruction(&Instruction::Else);
        self.emit_temporal_east_asian_proleptic_year(model, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    pub(super) fn emit_temporal_east_asian_model_days_in_month(
        &self,
        model: &EastAsianYearLocals,
        ordinal: I64Local,
        out: I64Local,
        function: &mut Function,
    ) {
        (model.month_mask).load(function);
        (ordinal).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(29));
        function.instruction(&Instruction::I64Add);
        (out).store(function);
    }

    fn emit_temporal_east_asian_model_month_prefix_i64(
        &self,
        model: &EastAsianYearLocals,
        ordinal: I64Local,
        function: &mut Function,
    ) {
        (ordinal).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(29));
        function.instruction(&Instruction::I64Mul);
        (model.month_mask).load(function);
        function.instruction(&Instruction::I64Const(1));
        (ordinal).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Popcnt);
        function.instruction(&Instruction::I64Add);
    }

    pub(super) fn emit_temporal_east_asian_model_epoch_days(
        &self,
        model: &EastAsianYearLocals,
        ordinal: I64Local,
        day: I64Local,
        out: I64Local,
        function: &mut Function,
    ) {
        self.emit_temporal_east_asian_model_month_prefix_i64(model, ordinal, function);
        (model.start).load(function);
        function.instruction(&Instruction::I64Add);
        (day).load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        (out).store(function);
    }

    pub(super) fn emit_temporal_east_asian_model_code_present_i32(
        &self,
        model: &EastAsianYearLocals,
        code: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I32Const(0));
        for candidate in TemporalCalendarMonthCode::ALL {
            if candidate.month_number() > 12 {
                continue;
            }
            (code).load(function);
            function.instruction(&Instruction::I64Const(candidate.encoding()));
            function.instruction(&Instruction::I64Eq);
            if candidate.is_leap() {
                (model.leap_ordinal).load(function);
                function.instruction(&Instruction::I64Const(candidate.month_number() + 1));
                function.instruction(&Instruction::I64Eq);
                function.instruction(&Instruction::I32And);
            }
            function.instruction(&Instruction::I32Or);
        }
    }

    /// A missing leap code constrains to its same-numbered regular month.
    pub(super) fn emit_temporal_east_asian_model_code_ordinal(
        &mut self,
        model: &EastAsianYearLocals,
        code: I64Local,
        out: I64Local,
        function: &mut Function,
    ) {
        let original = self.runtime_schema().reserve_i64_local(function);
        (code).load(function);
        (original).store(function);
        function.instruction(&Instruction::I64Const(0));
        (out).store(function);
        for candidate in TemporalCalendarMonthCode::ALL {
            if candidate.month_number() > 12 {
                continue;
            }
            (original).load(function);
            function.instruction(&Instruction::I64Const(candidate.encoding()));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            if candidate.is_leap() {
                (model.leap_ordinal).load(function);
                function.instruction(&Instruction::I64Const(candidate.month_number() + 1));
                function.instruction(&Instruction::I64Eq);
                function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
                (model.leap_ordinal).load(function);
                function.instruction(&Instruction::Else);
            }
            function.instruction(&Instruction::I64Const(candidate.month_number()));
            (model.leap_ordinal).load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I64Const(candidate.month_number()));
            (model.leap_ordinal).load(function);
            function.instruction(&Instruction::I64GeS);
            function.instruction(&Instruction::I32And);
            function.instruction(&Instruction::I64ExtendI32U);
            function.instruction(&Instruction::I64Add);
            if candidate.is_leap() {
                function.instruction(&Instruction::End);
            }
            (out).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.runtime_schema().release_i64_local(original, function);
    }

    fn emit_temporal_east_asian_model_month_code(
        &mut self,
        model: &EastAsianYearLocals,
        ordinal: I64Local,
        out: I64Local,
        function: &mut Function,
    ) {
        let number = self.runtime_schema().reserve_i64_local(function);
        let leap = self.runtime_schema().reserve_i64_local(function);
        (ordinal).load(function);
        (model.leap_ordinal).load(function);
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I64ExtendI32U);
        (leap).store(function);
        (ordinal).load(function);
        (model.leap_ordinal).load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        (ordinal).load(function);
        (model.leap_ordinal).load(function);
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Sub);
        (number).store(function);
        function.instruction(&Instruction::I64Const(0));
        (out).store(function);
        for candidate in TemporalCalendarMonthCode::ALL {
            if candidate.month_number() > 12 {
                continue;
            }
            (number).load(function);
            function.instruction(&Instruction::I64Const(candidate.month_number()));
            function.instruction(&Instruction::I64Eq);
            (leap).load(function);
            function.instruction(&Instruction::I64Const(i64::from(candidate.is_leap())));
            function.instruction(&Instruction::I64Eq);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(candidate.encoding()));
            (out).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.runtime_schema().release_i64_local(leap, function);
        self.runtime_schema().release_i64_local(number, function);
    }

    pub(in crate::builtins) fn emit_temporal_east_asian_days_in_month(
        &mut self,
        kind: TemporalEastAsianCalendar,
        year: I64Local,
        ordinal: I64Local,
        out: I64Local,
        function: &mut Function,
    ) {
        let model = self.emit_temporal_east_asian_year_model(kind, year, function);
        self.emit_temporal_east_asian_model_days_in_month(&model, ordinal, out, function);
        model.release(self, function);
    }

    pub(in crate::builtins) fn emit_temporal_east_asian_months_in_year(
        &mut self,
        kind: TemporalEastAsianCalendar,
        year: I64Local,
        out: I64Local,
        function: &mut Function,
    ) {
        let model = self.emit_temporal_east_asian_year_model(kind, year, function);
        (model.month_count).load(function);
        (out).store(function);
        model.release(self, function);
    }

    pub(in crate::builtins) fn emit_temporal_east_asian_month_code(
        &mut self,
        kind: TemporalEastAsianCalendar,
        year: I64Local,
        ordinal: I64Local,
        out: I64Local,
        function: &mut Function,
    ) {
        let model = self.emit_temporal_east_asian_year_model(kind, year, function);
        self.emit_temporal_east_asian_model_month_code(&model, ordinal, out, function);
        model.release(self, function);
    }

    pub(in crate::builtins) fn emit_temporal_east_asian_month_code_ordinal(
        &mut self,
        kind: TemporalEastAsianCalendar,
        year: I64Local,
        code: I64Local,
        out: I64Local,
        function: &mut Function,
    ) {
        let model = self.emit_temporal_east_asian_year_model(kind, year, function);
        self.emit_temporal_east_asian_model_code_ordinal(&model, code, out, function);
        model.release(self, function);
    }

    pub(in crate::builtins) fn emit_temporal_east_asian_month_code_present(
        &mut self,
        kind: TemporalEastAsianCalendar,
        year: I64Local,
        code: I64Local,
        function: &mut Function,
    ) {
        let model = self.emit_temporal_east_asian_year_model(kind, year, function);
        self.emit_temporal_east_asian_model_code_present_i32(&model, code, function);
        model.release(self, function);
    }

    pub(super) fn emit_temporal_east_asian_to_iso(
        &mut self,
        kind: TemporalEastAsianCalendar,
        fields: [I64Local; 3],
        function: &mut Function,
    ) {
        let model = self.emit_temporal_east_asian_year_model(kind, fields[0], function);
        let epoch = self.runtime_schema().reserve_i64_local(function);
        self.emit_temporal_east_asian_model_epoch_days(
            &model, fields[1], fields[2], epoch, function,
        );
        self.emit_temporal_civil_from_days(epoch, fields[0], fields[1], fields[2], function);
        self.runtime_schema().release_i64_local(epoch, function);
        model.release(self, function);
    }

    pub(super) fn emit_temporal_east_asian_project_date(
        &mut self,
        kind: TemporalEastAsianCalendar,
        iso: [I64Local; 3],
        result: &TemporalCalendarDateLocals,
        function: &mut Function,
    ) {
        let epoch = self.runtime_schema().reserve_i64_local(function);
        let native_year = self.runtime_schema().reserve_i64_local(function);
        self.emit_temporal_plain_date_epoch_days(iso[0], iso[1], iso[2], epoch, function);
        (iso[0]).load(function);
        (native_year).store(function);
        let model = self.emit_temporal_east_asian_year_model(kind, native_year, function);
        (epoch).load(function);
        (model.start).load(function);
        function.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, function);
        (native_year).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        (native_year).store(function);
        self.emit_temporal_east_asian_select_year(&model, native_year, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (model.year).load(function);
        (result.fields[0]).store(function);
        (epoch).load(function);
        (model.start).load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (result.day_of_year).store(function);
        function.instruction(&Instruction::I64Const(1));
        (result.fields[1]).store(function);
        for ordinal in 2_i64..=13 {
            (model.month_count).load(function);
            function.instruction(&Instruction::I64Const(ordinal));
            function.instruction(&Instruction::I64GeS);
            (result.day_of_year).load(function);
            function.instruction(&Instruction::I64Const(29 * (ordinal - 1)));
            (model.month_mask).load(function);
            function.instruction(&Instruction::I64Const((1 << (ordinal - 1)) - 1));
            function.instruction(&Instruction::I64And);
            function.instruction(&Instruction::I64Popcnt);
            function.instruction(&Instruction::I64Add);
            function.instruction(&Instruction::I64GtS);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(ordinal));
            (result.fields[1]).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        (result.day_of_year).load(function);
        self.emit_temporal_east_asian_model_month_prefix_i64(&model, result.fields[1], function);
        function.instruction(&Instruction::I64Sub);
        (result.fields[2]).store(function);
        self.emit_temporal_east_asian_model_days_in_month(
            &model,
            result.fields[1],
            result.days_in_month,
            function,
        );
        (model.next_start).load(function);
        (model.start).load(function);
        function.instruction(&Instruction::I64Sub);
        (result.days_in_year).store(function);
        (model.month_count).load(function);
        (result.months_in_year).store(function);
        (model.leap_ordinal).load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I64ExtendI32U);
        (result.leap).store(function);
        model.release(self, function);
        self.runtime_schema()
            .release_i64_local(native_year, function);
        self.runtime_schema().release_i64_local(epoch, function);
    }

    pub(super) fn emit_temporal_east_asian_month_serial(
        &mut self,
        kind: TemporalEastAsianCalendar,
        year: I64Local,
        ordinal: I64Local,
        out: I64Local,
        function: &mut Function,
    ) {
        let model = self.emit_temporal_east_asian_year_model(kind, year, function);
        (ordinal).load(function);
        (model.first_month_serial).load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        (out).store(function);
        model.release(self, function);
    }

    /// Retained prefixes are searched directly. Outside the image, a serial
    /// selects its mean-lunar moment, whose Gregorian year is the related year
    /// or its successor. The completed model then supplies the exact ordinal.
    pub(super) fn emit_temporal_east_asian_year_month_from_serial(
        &mut self,
        kind: TemporalEastAsianCalendar,
        serial: I64Local,
        year_out: I64Local,
        ordinal_out: I64Local,
        function: &mut Function,
    ) {
        let image = self.strings.temporal_east_asian_year_image().calendar(kind);
        let first = image.first_related_year();
        let count = i64::from(image.year_count());
        let total = image.total_months();
        let offsets = [image.lower_serial_offset(), image.upper_serial_offset()];
        let original = self.runtime_schema().reserve_i64_local(function);
        let native_year = self.runtime_schema().reserve_i64_local(function);
        (serial).load(function);
        (original).store(function);
        (original).load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64GeS);
        (original).load(function);
        function.instruction(&Instruction::I64Const(total));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let pointer = self.strings.temporal_east_asian_year_image().install_rows(
            kind,
            self.runtime_schema(),
            function,
        );
        let low = self.runtime_schema().reserve_i64_local(function);
        let high = self.runtime_schema().reserve_i64_local(function);
        let middle = self.runtime_schema().reserve_i64_local(function);
        let address = self.runtime_schema().reserve_i64_local(function);
        let prefix = self.runtime_schema().reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(0));
        (low).store(function);
        function.instruction(&Instruction::I64Const(count));
        (high).store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        (low).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (high).load(function);
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::BrIf(1));
        (low).load(function);
        (high).load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64DivS);
        (middle).store(function);
        (middle).load(function);
        function.instruction(&Instruction::I64Const(EAST_ASIAN_YEAR_ROW_BYTES as i64));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Const(i64::from(pointer)));
        function.instruction(&Instruction::I64Add);
        (address).store(function);
        (address).load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I64Load32S(Self::memarg32(
            TemporalEastAsianYearRowSlot::MonthPrefix.byte_offset(),
        )));
        (prefix).store(function);
        (original).load(function);
        (prefix).load(function);
        function.instruction(&Instruction::I64GeS);
        self.open_frame(ControlFrameKind::If, function);
        (middle).load(function);
        (low).store(function);
        function.instruction(&Instruction::Else);
        (middle).load(function);
        (high).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        (low).load(function);
        function.instruction(&Instruction::I64Const(first));
        function.instruction(&Instruction::I64Add);
        (native_year).store(function);
        for local in [prefix, address, middle, high, low] {
            self.runtime_schema().release_i64_local(local, function);
        }
        function.instruction(&Instruction::Else);
        let index = self.runtime_schema().reserve_i64_local(function);
        let epoch = self.runtime_schema().reserve_i64_local(function);
        let iso_month = self.runtime_schema().reserve_i64_local(function);
        let iso_day = self.runtime_schema().reserve_i64_local(function);
        (original).load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, function);
        (original).load(function);
        function.instruction(&Instruction::I64Const(offsets[0]));
        function.instruction(&Instruction::I64Sub);
        (index).store(function);
        self.emit_temporal_east_asian_moon_epoch(kind, index, true, epoch, function);
        function.instruction(&Instruction::Else);
        (original).load(function);
        function.instruction(&Instruction::I64Const(offsets[1]));
        function.instruction(&Instruction::I64Sub);
        (index).store(function);
        self.emit_temporal_east_asian_moon_epoch(kind, index, false, epoch, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_temporal_civil_from_days(epoch, native_year, iso_month, iso_day, function);
        for local in [iso_day, iso_month, epoch, index] {
            self.runtime_schema().release_i64_local(local, function);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let model = self.emit_temporal_east_asian_year_model(kind, native_year, function);
        (original).load(function);
        (model.first_month_serial).load(function);
        function.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, function);
        (native_year).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        (native_year).store(function);
        self.emit_temporal_east_asian_select_year(&model, native_year, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (original).load(function);
        (model.first_month_serial).load(function);
        (model.month_count).load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64GeS);
        self.open_frame(ControlFrameKind::If, function);
        (native_year).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (native_year).store(function);
        self.emit_temporal_east_asian_select_year(&model, native_year, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (model.year).load(function);
        (year_out).store(function);
        (original).load(function);
        (model.first_month_serial).load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (ordinal_out).store(function);
        model.release(self, function);
        self.runtime_schema()
            .release_i64_local(native_year, function);
        self.runtime_schema().release_i64_local(original, function);
    }

    pub(super) fn emit_temporal_east_asian_year_intersects_iso_limits_i32(
        &mut self,
        kind: TemporalEastAsianCalendar,
        year: I64Local,
        function: &mut Function,
    ) {
        let model = self.emit_temporal_east_asian_year_model(kind, year, function);
        (model.start).load(function);
        function.instruction(&Instruction::I64Const(
            TEMPORAL_PLAIN_DATE_MAXIMUM_EPOCH_DAY,
        ));
        function.instruction(&Instruction::I64LeS);
        (model.next_start).load(function);
        function.instruction(&Instruction::I64Const(
            TEMPORAL_PLAIN_DATE_MINIMUM_EPOCH_DAY,
        ));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32And);
        model.release(self, function);
    }
}
