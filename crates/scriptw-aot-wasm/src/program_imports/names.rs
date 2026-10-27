use super::*;

pub(super) fn rewrite(
    encoder: &mut Rewrite<'_>,
    names: &mut wasm_encoder::NameSection,
    name: wasmparser::Name<'_>,
) -> Result<(), reencode::Error<String>> {
    use wasmparser::Name;
    match name {
        Name::Function(reader) => {
            let mut map = wasm_encoder::NameMap::new();
            for row in reader {
                let row = row?;
                if let Some(index) = encoder
                    .maps
                    .functions
                    .get(FunctionIndex(row.index))
                    .map_err(user)?
                {
                    map.append(index.0, row.name);
                }
            }
            names.functions(&map);
        }
        Name::Global(reader) => {
            let mut map = wasm_encoder::NameMap::new();
            for row in reader {
                let row = row?;
                if let Some(index) = encoder
                    .maps
                    .globals
                    .get(GlobalIndex(row.index))
                    .map_err(user)?
                {
                    map.append(index.0, row.name);
                }
            }
            names.globals(&map);
        }
        Name::Local(reader) => indirect(encoder, names, reader, false)?,
        Name::Label(reader) => indirect(encoder, names, reader, true)?,
        other @ (Name::Module { .. }
        | Name::Type(_)
        | Name::Table(_)
        | Name::Memory(_)
        | Name::Element(_)
        | Name::Data(_)
        | Name::Tag(_)
        | Name::Field(_)) => {
            reencode::utils::parse_custom_name_subsection(encoder, names, other)?;
        }
        Name::Unknown { ty, .. } => {
            return Err(user(format!("unsupported compiler name subsection {ty}")))
        }
    }
    Ok(())
}
fn indirect(
    encoder: &mut Rewrite<'_>,
    names: &mut wasm_encoder::NameSection,
    reader: wasmparser::IndirectNameMap<'_>,
    labels: bool,
) -> Result<(), reencode::Error<String>> {
    let mut map = wasm_encoder::IndirectNameMap::new();
    for row in reader {
        let row = row?;
        let Some(index) = encoder
            .maps
            .functions
            .get(FunctionIndex(row.index))
            .map_err(user)?
        else {
            continue;
        };
        let mut inner = wasm_encoder::NameMap::new();
        for entry in row.names {
            let entry = entry?;
            inner.append(entry.index, entry.name);
        }
        map.append(index.0, &inner);
    }
    if labels {
        names.labels(&map);
    } else {
        names.locals(&map);
    }
    Ok(())
}
