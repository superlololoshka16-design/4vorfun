use compact_str::CompactString;
use smallvec::SmallVec;
use std::io::Write as _;
use std::sync::OnceLock;
use wasmtime::{Config, Engine, Linker, Module, Store, ValType};

static ENGINE: OnceLock<Engine> = OnceLock::new();

fn engine() -> &'static Engine {
    ENGINE.get_or_init(|| {
        let mut cfg = Config::new();
        cfg.consume_fuel(true);
        Engine::new(&cfg).expect("wasmtime engine")
    })
}

fn err_msg(msg: &str) -> wasmtime::Error {
    wasmtime::Error::msg(msg.to_owned())
}

pub fn run_wasm(bin: &[u8], fuel: u64) -> Result<CompactString, wasmtime::Error> {
    let engine = engine();
    let module = Module::new(engine, bin)?;
    if module.imports().next().is_some() {
        return Err(err_msg("imports unsupported"));
    }
    let mut store = Store::new(engine, ());
    store.set_fuel(fuel)?;
    let linker = Linker::new(engine);
    let instance = linker.instantiate(&mut store, &module)?;
    let mut name: Option<&str> = None;
    let mut ty: Option<ValType> = None;
    for export in module.exports() {
        if let wasmtime::ExternType::Func(ft) = export.ty()
            && ft.params().next().is_none()
            && ft.results().len() == 1
        {
            name = Some(export.name());
            ty = ft.results().next();
            break;
        }
    }
    let name = name.ok_or_else(|| err_msg("no entry export"))?;
    let ty = ty.expect("checked");
    match ty {
        ValType::I32 => {
            let f = instance.get_typed_func::<(), i32>(&mut store, name)?;
            let v = f.call(&mut store, ())?;
            Ok(int_token(v as i64))
        }
        ValType::I64 => {
            let f = instance.get_typed_func::<(), i64>(&mut store, name)?;
            let v = f.call(&mut store, ())?;
            Ok(int_token(v))
        }
        ValType::F32 => {
            let f = instance.get_typed_func::<(), f32>(&mut store, name)?;
            let v = f.call(&mut store, ())?;
            Ok(float_token(v as f64))
        }
        ValType::F64 => {
            let f = instance.get_typed_func::<(), f64>(&mut store, name)?;
            let v = f.call(&mut store, ())?;
            Ok(float_token(v))
        }
        _ => Err(err_msg("unsupported result type")),
    }
}

fn int_token(bits: i64) -> CompactString {
    let mut buf: SmallVec<[u8; 24]> = SmallVec::new();
    let _ = write!(&mut buf, "{bits}");
    CompactString::from_utf8_lossy(&buf)
}

fn float_token(v: f64) -> CompactString {
    let mut buf: SmallVec<[u8; 32]> = SmallVec::new();
    let _ = write!(&mut buf, "{v}");
    CompactString::from_utf8_lossy(&buf)
}
