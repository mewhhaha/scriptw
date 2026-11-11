//! The real objects stay inside the program. Only explicit primitive
//! projections and hook events enter the existing completion/print observer.
use super::*;

// Exactly one Realm is created, independently of the action count. No action
// mutates its intrinsics or depends on another action to establish a binding.
pub(super) const REALM_SETUP: &str = "const scenario_other=__scriptwCreateRealm().global;const scenario_foreign_map=new scenario_other.Map();\n";
pub(super) const TEMPORAL_SETUP: &str = "let scenario_date=new Temporal.PlainDate(2000,1,31);let scenario_instant=new Temporal.Instant(0n);let scenario_zoned=new Temporal.ZonedDateTime(0n,'+00:00');\n";

pub(super) fn realm(
    source: &mut String,
    operation: RealmOperation,
    slot: u8,
    value: i8,
    state: &str,
) {
    match operation {
        RealmOperation::ArrayMap => {
            writeln!(source, "const result=scenario_other.Array.prototype.map.call([{slot},{value}],(item,index)=>{{print('realm-map-hook:'+index+':'+item);return item+1;}});print('realm-map:'+(Object.getPrototypeOf(result)===scenario_other.Array.prototype)+':'+result.join('|'));{state}=({state}+result[0]+result[1])|0;").unwrap();
        }
        RealmOperation::ArraySpecies => {
            writeln!(source, "const items=new scenario_other.Array({slot},{value});Object.defineProperty(items,'constructor',{{value:{{get [Symbol.species](){{print('realm-species');return scenario_other.Array;}}}}}});const result=Array.prototype.slice.call(items);print('realm-slice:'+(Object.getPrototypeOf(result)===scenario_other.Array.prototype)+':'+result.join('|'));{state}=({state}+result.length+result[1])|0;").unwrap();
        }
        RealmOperation::BoxPrimitive => {
            writeln!(source, "const boxed=scenario_other.Object({value});const primitive=Number.prototype.valueOf.call(boxed);print('realm-box:'+(Object.getPrototypeOf(boxed)===scenario_other.Number.prototype)+':'+primitive);{state}=({state}+primitive)|0;").unwrap();
        }
        RealmOperation::ErrorRealm => {
            writeln!(source, "try{{scenario_other.Array.prototype.map.call(null,item=>item+{value});print('realm-error-missing');}}catch(error){{const foreign=Object.getPrototypeOf(error)===scenario_other.TypeError.prototype;print('realm-error:'+foreign+':'+(Object.getPrototypeOf(error)===TypeError.prototype));{state}=({state}+(foreign?1:0))|0;}}").unwrap();
        }
        RealmOperation::MapBrand => {
            writeln!(source, "const returned=Map.prototype.set.call(scenario_foreign_map,{slot},{value});const item=scenario_other.Map.prototype.get.call(scenario_foreign_map,{slot});print('realm-map-brand:'+(returned===scenario_foreign_map)+':'+item+':'+scenario_foreign_map.size);{state}=({state}+item+scenario_foreign_map.size)|0;").unwrap();
        }
        RealmOperation::TypedArray => {
            writeln!(source, "const items=new scenario_other.Uint8Array([{slot},{value}]);const result=Uint8Array.prototype.slice.call(items);print('realm-typed:'+(Object.getPrototypeOf(result)===scenario_other.Uint8Array.prototype)+':'+result.join('|'));{state}=({state}+result[1])|0;").unwrap();
        }
        RealmOperation::ReflectConstruct => {
            writeln!(source, "function scenario_new_target(){{}}scenario_new_target.prototype=new scenario_other.Object();const result=scenario_other.Reflect.construct(scenario_other.Array,[{slot}],scenario_new_target);print('realm-construct:'+(Object.getPrototypeOf(result)===scenario_new_target.prototype)+':'+Array.isArray(result)+':'+result.length);{state}=({state}+result.length)|0;").unwrap();
        }
        RealmOperation::AbruptIdentity => {
            writeln!(source, "const marker=new scenario_other.Object();marker.value={value};const items={{get length(){{print('realm-length');throw marker;}},get 0(){{print('realm-late-index');return {slot};}}}};try{{scenario_other.Array.prototype.map.call(items,item=>{{print('realm-late-callback');return item;}});print('realm-abrupt-missing');}}catch(error){{print('realm-abrupt:'+(error===marker));{state}=({state}+(error===marker?marker.value:0))|0;}}").unwrap();
        }
    }
}

pub(super) fn temporal(
    source: &mut String,
    operation: TemporalOperation,
    slot: u8,
    value: i8,
    state: &str,
) {
    match operation {
        TemporalOperation::DateAdd => {
            writeln!(source, "scenario_date=scenario_date.add({{days:{value}}});print('temporal-date:'+scenario_date.year+':'+scenario_date.month+':'+scenario_date.day+':'+scenario_date.calendarId);{state}=({state}+scenario_date.day)|0;").unwrap();
        }
        TemporalOperation::DateWith => {
            writeln!(source, "scenario_date=scenario_date.with({{month:{},day:{}}},{{get overflow(){{print('temporal-overflow');return 'constrain';}}}});print('temporal-with:'+scenario_date.year+':'+scenario_date.month+':'+scenario_date.day);{state}=({state}+scenario_date.month+scenario_date.day)|0;", slot + 1, 28 + value.unsigned_abs() % 4).unwrap();
        }
        TemporalOperation::InstantAdd => {
            writeln!(source, "const previous=scenario_instant;scenario_instant=scenario_instant.add({{nanoseconds:{value}}});print('temporal-instant:'+scenario_instant.epochNanoseconds.toString()+':'+Temporal.Instant.compare(previous,scenario_instant));{state}=({state}+Number(scenario_instant.epochNanoseconds))|0;").unwrap();
        }
        TemporalOperation::ZonedAdd => {
            let zone = if slot % 2 == 0 { "+01:00" } else { "-01:00" };
            writeln!(source, "scenario_zoned=scenario_zoned.withTimeZone('{zone}').add({{days:{value}}});print('temporal-zoned:'+scenario_zoned.epochNanoseconds.toString()+':'+scenario_zoned.year+':'+scenario_zoned.month+':'+scenario_zoned.day+':'+scenario_zoned.hour+':'+scenario_zoned.offset);{state}=({state}+scenario_zoned.day+scenario_zoned.hour)|0;").unwrap();
        }
        TemporalOperation::DurationRound => {
            let increment = [1, 2, 3, 4, 5, 6, 10, 12][usize::from(slot)];
            writeln!(source, "const duration=Temporal.Duration.from({{days:{value},minutes:{value},seconds:{value}}});const rounded=duration.round({{smallestUnit:'minute',roundingIncrement:{increment},relativeTo:scenario_date}});print('temporal-round:'+rounded.days+':'+rounded.hours+':'+rounded.minutes+':'+rounded.seconds);{state}=({state}+rounded.days+rounded.minutes)|0;").unwrap();
        }
        TemporalOperation::DurationTotal => {
            writeln!(source, "const duration=Temporal.Duration.from({{days:{value},hours:{value}}});const total=duration.total({{unit:'hour',relativeTo:scenario_zoned}});const comparison=Temporal.Duration.compare(duration,{{hours:{value}}},{{relativeTo:scenario_date}});print('temporal-total:'+total+':'+comparison);{state}=({state}+total+comparison)|0;").unwrap();
        }
        TemporalOperation::ConversionOrder => {
            writeln!(source, "const fields={{get calendar(){{print('temporal-get-calendar');return 'iso8601';}},get year(){{print('temporal-get-year');return 2000;}},get month(){{print('temporal-get-month');return 1;}},get day(){{print('temporal-get-day');return {};}},get hour(){{print('temporal-get-hour');return {};}},get timeZone(){{print('temporal-get-zone');return '+00:00';}}}};const options={{get disambiguation(){{print('temporal-get-disambiguation');return 'compatible';}},get offset(){{print('temporal-get-offset');return 'reject';}},get overflow(){{print('temporal-get-overflow');return 'constrain';}}}};scenario_zoned=Temporal.ZonedDateTime.from(fields,options);print('temporal-convert:'+scenario_zoned.epochNanoseconds.toString()+':'+scenario_zoned.day+':'+scenario_zoned.hour);{state}=({state}+scenario_zoned.day+scenario_zoned.hour)|0;", slot + 1, value.unsigned_abs()).unwrap();
        }
        TemporalOperation::AbruptIdentity => {
            writeln!(source, "const marker={{value:{value}}};const fields={{get calendar(){{print('temporal-abrupt-calendar');return 'iso8601';}},get day(){{print('temporal-abrupt-day');throw marker;}},get month(){{print('temporal-late-month');return 1;}},get year(){{print('temporal-late-year');return 2000;}}}};try{{Temporal.PlainDate.from(fields,{{get overflow(){{print('temporal-late-overflow');return 'constrain';}}}});print('temporal-abrupt-missing');}}catch(error){{print('temporal-abrupt:'+(error===marker));{state}=({state}+(error===marker?marker.value:0))|0;}}").unwrap();
        }
    }
}
