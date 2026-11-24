use wasmparser::{
    BinaryReader, CompositeInnerType, DataKind, ElementItems, ElementKind, FuncType, FunctionBody,
    Operator, Parser, Payload, TypeRef, ValidPayload, Validator, WasmFeatures,
};

pub(crate) const DEPTH_EXPORT: &str = "aether:depth";
pub(crate) const DEPTH_LIMIT: u32 = 1 << 16;
pub(crate) const MAX_FRAME: u32 = 1 << 14;
const FRAME_BASE: u32 = 8;
const PROBE_SLOTS: u32 = 2;

const SECTION_GLOBAL: u8 = 6;
const SECTION_EXPORT: u8 = 7;
const SECTION_CODE: u8 = 10;

pub(crate) fn features() -> WasmFeatures {
    WasmFeatures::WASM2.difference(WasmFeatures::GC_TYPES)
}

pub(crate) struct Metered {
    pub wasm: Vec<u8>,
    pub init_fuel: u64,
}

struct Analysis {
    init_fuel: u64,
    imported_funcs: u32,
    depth_global: u32,
    costs: Vec<u32>,
    indirect_costs: Vec<u32>,
}

fn analyze(wasm: &[u8]) -> Result<Analysis, String> {
    let mut validator = Validator::new_with_features(features());
    let mut types: Vec<FuncType> = Vec::new();
    let mut func_types: Vec<u32> = Vec::new();
    let mut imported_funcs: u32 = 0;
    let mut globals: u32 = 0;
    let mut init_fuel: u64 = 0;
    let mut costs = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        let payload = payload.map_err(|e| e.to_string())?;
        match &payload {
            Payload::TypeSection(r) => {
                for group in r.clone() {
                    for sub in group.map_err(|e| e.to_string())?.into_types() {
                        match sub.composite_type.inner {
                            CompositeInnerType::Func(f) => types.push(f),
                            _ => return Err("only function types are supported".into()),
                        }
                    }
                }
            }
            Payload::ImportSection(r) => {
                for import in r.clone().into_imports() {
                    match import.map_err(|e| e.to_string())?.ty {
                        TypeRef::Func(t) => {
                            func_types.push(t);
                            imported_funcs += 1;
                        }
                        TypeRef::Global(_) => globals += 1,
                        _ => {}
                    }
                }
            }
            Payload::FunctionSection(r) => {
                for t in r.clone() {
                    func_types.push(t.map_err(|e| e.to_string())?);
                }
            }
            Payload::GlobalSection(r) => globals += r.count(),
            Payload::DataSection(r) => {
                for data in r.clone() {
                    let data = data.map_err(|e| e.to_string())?;
                    if matches!(data.kind, DataKind::Active { .. }) {
                        init_fuel += data.data.len() as u64;
                    }
                }
            }
            Payload::ElementSection(r) => {
                for element in r.clone() {
                    let element = element.map_err(|e| e.to_string())?;
                    if matches!(element.kind, ElementKind::Active { .. }) {
                        init_fuel += u64::from(match &element.items {
                            ElementItems::Functions(r) => r.count(),
                            ElementItems::Expressions(_, r) => r.count(),
                        });
                    }
                }
            }
            Payload::StartSection { .. } => {
                return Err("capsules may not declare a start function; do setup in tick".into());
            }
            Payload::ExportSection(r) => {
                for export in r.clone() {
                    if export.map_err(|e| e.to_string())?.name == DEPTH_EXPORT {
                        return Err(format!("the export name {DEPTH_EXPORT:?} is reserved"));
                    }
                }
            }
            _ => {}
        }
        if let ValidPayload::Func(func, body) =
            validator.payload(&payload).map_err(|e| e.to_string())?
        {
            let mut v = func.into_validator(Default::default());
            for local in body.get_locals_reader().map_err(|e| e.to_string())? {
                let (count, ty) = local.map_err(|e| e.to_string())?;
                v.define_locals(0, count, ty).map_err(|e| e.to_string())?;
            }
            let mut ops = body.get_operators_reader().map_err(|e| e.to_string())?;
            let mut max_stack = 0;
            while !ops.eof() {
                let (op, offset) = ops.read_with_offset().map_err(|e| e.to_string())?;
                v.op(offset, &op).map_err(|e| e.to_string())?;
                max_stack = max_stack.max(v.operand_stack_height());
            }
            let cost = FRAME_BASE
                .saturating_add(v.len_locals())
                .saturating_add(max_stack)
                .saturating_add(PROBE_SLOTS);
            if cost > MAX_FRAME {
                return Err(format!(
                    "function {} needs a frame of {cost} slots; the limit is {MAX_FRAME}",
                    v.index()
                ));
            }
            costs.push(cost);
        }
    }

    let signature = |t: u32| types.get(t as usize);
    let indirect_costs = (0..types.len() as u32)
        .map(|t| {
            costs
                .iter()
                .enumerate()
                .filter(|(f, _)| signature(func_types[imported_funcs as usize + f]) == signature(t))
                .map(|(_, c)| *c)
                .fold(FRAME_BASE, u32::max)
        })
        .collect();
    Ok(Analysis {
        init_fuel,
        imported_funcs,
        depth_global: globals,
        costs,
        indirect_costs,
    })
}

fn uleb(out: &mut Vec<u8>, mut v: u32) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn sleb(out: &mut Vec<u8>, mut v: i32) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        let done = (v == 0 && byte & 0x40 == 0) || (v == -1 && byte & 0x40 != 0);
        out.push(if done { byte } else { byte | 0x80 });
        if done {
            return;
        }
    }
}

fn adjust(out: &mut Vec<u8>, global: u32, cost: u32, add: bool) {
    out.push(0x23);
    uleb(out, global);
    out.push(0x41);
    sleb(out, cost as i32);
    out.push(if add { 0x6a } else { 0x6b });
    out.push(0x24);
    uleb(out, global);
}

fn enter(out: &mut Vec<u8>, global: u32, cost: u32) {
    adjust(out, global, cost, true);
    out.push(0x23);
    uleb(out, global);
    out.push(0x41);
    sleb(out, DEPTH_LIMIT as i32);
    out.extend_from_slice(&[0x4b, 0x04, 0x40, 0x00, 0x0b]);
}

fn section(out: &mut Vec<u8>, id: u8, data: &[u8]) {
    out.push(id);
    uleb(out, data.len() as u32);
    out.extend_from_slice(data);
}

fn append_entry(data: &[u8], entry: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = BinaryReader::new(data, 0);
    let count = reader.read_var_u32().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(data.len() + entry.len() + 5);
    uleb(&mut out, count + 1);
    out.extend_from_slice(&data[reader.current_position()..]);
    out.extend_from_slice(entry);
    Ok(out)
}

fn instrument_body(a: &Analysis, body: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    let fb = FunctionBody::new(BinaryReader::new(body, 0));
    let mut ops = fb.get_operators_reader().map_err(|e| e.to_string())?;
    let start = ops.original_position() as usize;
    let mut code = body[..start].to_vec();
    while !ops.eof() {
        let (op, offset) = ops.read_with_offset().map_err(|e| e.to_string())?;
        let raw = &body[offset as usize..ops.original_position() as usize];
        let cost = match op {
            Operator::Call { function_index } if function_index >= a.imported_funcs => {
                Some(a.costs[(function_index - a.imported_funcs) as usize])
            }
            Operator::CallIndirect { type_index, .. } => {
                Some(a.indirect_costs[type_index as usize])
            }
            Operator::ReturnCall { .. }
            | Operator::ReturnCallIndirect { .. }
            | Operator::CallRef { .. }
            | Operator::ReturnCallRef { .. } => {
                return Err("tail calls and typed function references are not supported".into());
            }
            _ => None,
        };
        match cost {
            Some(cost) => {
                enter(&mut code, a.depth_global, cost);
                code.extend_from_slice(raw);
                adjust(&mut code, a.depth_global, cost, false);
            }
            None => code.extend_from_slice(raw),
        }
    }
    uleb(out, code.len() as u32);
    out.extend_from_slice(&code);
    Ok(())
}

fn instrument_code(a: &Analysis, data: &[u8]) -> Result<Vec<u8>, String> {
    let mut reader = BinaryReader::new(data, 0);
    let count = reader.read_var_u32().map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(data.len() * 2);
    uleb(&mut out, count);
    for _ in 0..count {
        let size = reader.read_var_u32().map_err(|e| e.to_string())? as usize;
        let body = reader.read_bytes(size).map_err(|e| e.to_string())?;
        instrument_body(a, body, &mut out)?;
    }
    Ok(out)
}

fn order(id: u8) -> u8 {
    match id {
        13 => 6,
        6..=9 => id + 1,
        12 => 11,
        10 | 11 => id + 2,
        _ => id,
    }
}

pub(crate) fn instrument(wasm: &[u8]) -> Result<Metered, String> {
    let a = analyze(wasm)?;
    let global_entry = [0x7f, 0x01, 0x41, 0x00, 0x0b];
    let mut export_entry = Vec::new();
    uleb(&mut export_entry, DEPTH_EXPORT.len() as u32);
    export_entry.extend_from_slice(DEPTH_EXPORT.as_bytes());
    export_entry.push(0x03);
    uleb(&mut export_entry, a.depth_global);

    let mut out = wasm[..8].to_vec();
    let mut reader = BinaryReader::new(wasm, 0);
    reader.read_bytes(8).map_err(|e| e.to_string())?;
    let mut global_done = false;
    let mut export_done = false;
    while !reader.eof() {
        let id = reader.read_u8().map_err(|e| e.to_string())?;
        let size = reader.read_var_u32().map_err(|e| e.to_string())? as usize;
        let data = reader.read_bytes(size).map_err(|e| e.to_string())?;
        if id != 0 && !global_done && order(id) > order(SECTION_GLOBAL) {
            let mut fresh = vec![1];
            fresh.extend_from_slice(&global_entry);
            section(&mut out, SECTION_GLOBAL, &fresh);
            global_done = true;
        }
        match id {
            SECTION_GLOBAL => {
                section(&mut out, id, &append_entry(data, &global_entry)?);
                global_done = true;
            }
            SECTION_EXPORT => {
                section(&mut out, id, &append_entry(data, &export_entry)?);
                export_done = true;
            }
            SECTION_CODE => section(&mut out, id, &instrument_code(&a, data)?),
            _ => section(&mut out, id, data),
        }
    }
    if !export_done {
        return Err("module must export `tick`".into());
    }
    Ok(Metered {
        wasm: out,
        init_fuel: a.init_fuel,
    })
}

#[cfg(test)]
mod tests {
    use arbitrary::Unstructured;
    use wasmtime::{Config, Engine, Instance, Module, Store, Trap, Val};

    use super::*;

    fn smith_config() -> wasm_smith::Config {
        wasm_smith::Config {
            allow_start_export: false,
            exceptions_enabled: false,
            gc_enabled: false,
            compact_imports_enabled: false,
            memory64_enabled: false,
            relaxed_simd_enabled: false,
            tail_call_enabled: false,
            threads_enabled: false,
            wide_arithmetic_enabled: false,
            extended_const_enabled: false,
            max_imports: 0,
            max_memory32_bytes: 1 << 20,
            max_table_elements: 1000,
            export_everything: true,
            canonicalize_nans: true,
            disallow_traps: true,
            min_funcs: 3,
            min_exports: 3,
            reference_types_enabled: false,
            ..Default::default()
        }
    }

    fn bits(v: &Val) -> String {
        match v {
            Val::I32(x) => format!("i32:{x}"),
            Val::I64(x) => format!("i64:{x}"),
            Val::F32(x) => format!("f32:{x:#x}"),
            Val::F64(x) => format!("f64:{x:#x}"),
            Val::V128(x) => format!("v128:{:#x}", x.as_u128()),
            other => format!("{other:?}").chars().take(4).collect(),
        }
    }

    enum Outcome {
        Values(Vec<String>),
        Trap,
        Inconclusive,
    }

    fn call_all(engine: &Engine, wasm: &[u8], depth_export: bool) -> Vec<(String, Outcome)> {
        let module = Module::from_binary(engine, wasm).expect("module validates");
        let mut store = Store::new(engine, ());
        store.set_fuel(u64::MAX).unwrap();
        let Ok(instance) = Instance::new(&mut store, &module, &[]) else {
            return Vec::new();
        };
        let names: Vec<String> = module
            .exports()
            .filter(|e| e.ty().func().is_some())
            .map(|e| e.name().to_owned())
            .collect();
        let mut out = Vec::new();
        for name in names {
            let func = instance.get_func(&mut store, &name).unwrap();
            let ty = func.ty(&store);
            let Some(params) = ty
                .params()
                .map(|t| t.default_value())
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let mut results: Vec<Val> = ty.results().map(|_| Val::I32(0)).collect();
            store.set_fuel(200_000).unwrap();
            let outcome = match func.call(&mut store, &params, &mut results) {
                Ok(()) => Outcome::Values(results.iter().map(bits).collect()),
                Err(e) => {
                    let depth_hit = depth_export
                        && instance
                            .get_global(&mut store, DEPTH_EXPORT)
                            .and_then(|g| g.get(&mut store).i32())
                            .is_some_and(|d| d.unsigned_abs() > DEPTH_LIMIT);
                    match e.downcast_ref::<Trap>() {
                        Some(Trap::OutOfFuel) | Some(Trap::StackOverflow) => Outcome::Inconclusive,
                        _ if depth_hit => Outcome::Inconclusive,
                        _ => Outcome::Trap,
                    }
                }
            };
            if let Some(g) = depth_export
                .then(|| instance.get_global(&mut store, DEPTH_EXPORT))
                .flatten()
            {
                g.set(&mut store, Val::I32(0)).unwrap();
            }
            out.push((name, outcome));
        }
        out
    }

    #[test]
    fn instrumented_modules_validate_and_behave_like_the_original() {
        let mut config = Config::new();
        config
            .consume_fuel(true)
            .cranelift_nan_canonicalization(true);
        let engine = Engine::new(&config).unwrap();
        let mut seed = 0x5eed_u64;
        let (mut accepted, mut rejected, mut compared, mut no_calls, mut inconclusive) =
            (0, 0, 0, 0, 0);
        for _ in 0..1500 {
            let bytes: Vec<u8> = (0..4096)
                .map(|_| {
                    seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
                    let mut z = seed;
                    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                    (z ^ (z >> 27)) as u8
                })
                .collect();
            let Ok(module) =
                wasm_smith::Module::new(smith_config(), &mut Unstructured::new(&bytes))
            else {
                continue;
            };
            let original = module.to_bytes();
            let Ok(Metered { wasm: metered, .. }) = instrument(&original) else {
                rejected += 1;
                continue;
            };
            accepted += 1;
            Validator::new_with_features(features())
                .validate_all(&metered)
                .expect("instrumented module is valid");
            let before = call_all(&engine, &original, false);
            if before.is_empty() {
                no_calls += 1;
            }
            inconclusive += before
                .iter()
                .filter(|(_, o)| matches!(o, Outcome::Inconclusive))
                .count();
            let after = call_all(&engine, &metered, true);
            assert_eq!(before.len(), after.len());
            for ((name, a), (_, b)) in before.iter().zip(&after) {
                match (a, b) {
                    (Outcome::Inconclusive, _) | (_, Outcome::Inconclusive) => {}
                    (Outcome::Values(x), Outcome::Values(y)) => {
                        assert_eq!(x, y, "{name} returned differently");
                        compared += 1;
                    }
                    (Outcome::Trap, Outcome::Trap) => compared += 1,
                    _ => panic!("{name} trapped in only one version"),
                }
            }
        }
        eprintln!(
            "accepted {accepted}, rejected {rejected}, compared calls {compared}, modules without calls {no_calls}, inconclusive {inconclusive}"
        );
        assert!(
            accepted >= 1000,
            "only {accepted} generated modules were accepted"
        );
        assert!(compared >= 1000, "only {compared} calls were compared");
    }
}
