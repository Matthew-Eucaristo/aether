use std::collections::HashMap;

use wasmtime::{Config, Engine, ExternType, InstancePre, Linker, Module};

use crate::blob::{BlobHash, Blobs};
use crate::host::{self, HostState};
use crate::kernel::KernelError;
use crate::meter;

pub(crate) const WASM_STACK: usize = 16 << 20;
pub(crate) const THREAD_STACK: usize = WASM_STACK + (8 << 20);

#[derive(Clone)]
pub(crate) struct Prepared {
    pub pre: InstancePre<HostState>,
    pub init_fuel: u64,
}

pub(crate) struct Runtime {
    engine: Engine,
    linker: Linker<HostState>,
    cache: HashMap<BlobHash, Prepared>,
}

impl Runtime {
    pub fn new() -> Self {
        let mut config = Config::new();
        config
            .consume_fuel(true)
            .cranelift_nan_canonicalization(true)
            .async_stack_size(WASM_STACK)
            .max_wasm_stack(WASM_STACK);
        let engine = Engine::new(&config).expect("static wasmtime config is valid");
        let linker = host::linker(&engine).expect("host functions have unique names");
        Self {
            engine,
            linker,
            cache: HashMap::new(),
        }
    }

    pub fn forget(&mut self, hash: &BlobHash) {
        self.cache.remove(hash);
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn prepare(&mut self, blobs: &Blobs, hash: &BlobHash) -> Result<Prepared, KernelError> {
        if let Some(pre) = self.cache.get(hash) {
            return Ok(pre.clone());
        }
        let bytes = blobs.get(hash).ok_or(KernelError::MissingBlob(*hash))?;
        let binary =
            wat::parse_bytes(bytes).map_err(|e| KernelError::InvalidModule(e.to_string()))?;
        let metered = meter::instrument(&binary).map_err(KernelError::InvalidModule)?;
        let module = Module::from_binary(&self.engine, &metered.wasm)
            .map_err(|e| KernelError::InvalidModule(e.to_string()))?;
        let exports_tick = module.exports().any(|e| {
            e.name() == "tick"
                && matches!(e.ty(), ExternType::Func(f) if f.params().len() == 0 && f.results().len() == 0)
        });
        if !exports_tick {
            return Err(KernelError::InvalidModule(
                "module must export `tick` with no params and no results".into(),
            ));
        }
        let pre = self
            .linker
            .instantiate_pre(&module)
            .map_err(|e| KernelError::InvalidModule(e.to_string()))?;
        let prepared = Prepared {
            pre,
            init_fuel: metered.init_fuel,
        };
        self.cache.insert(*hash, prepared.clone());
        Ok(prepared)
    }
}
