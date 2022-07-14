//! This module defines the engine and its components.
//!
//! This engine uses a register machine based bytecode.

mod bytecode;
mod code_map;
mod config;
mod const_pool;
mod func_args;
mod func_builder;
mod func_types;
mod ident;
mod inner;
mod provider;
mod traits;

#[cfg(test)]
mod tests;

pub(crate) use self::{
    bytecode::{ExecInstruction, ExecRegisterSlice, Instruction, InstructionTypes, Target},
    func_args::{FuncParams, FuncResults},
    func_builder::{FunctionBuilder, IrProvider, IrRegister},
    provider::{DedupProviderSliceArena, ExecProvider, ExecProviderSlice},
    traits::{CallParams, CallResults},
};
use self::{
    bytecode::{ExecRegister, Offset},
    code_map::CodeMap,
    func_builder::{CompileContext, IrInstruction},
    func_types::FuncTypeRegistry,
    ident::{EngineIdent, Guarded},
    inner::EngineInner,
};
pub use self::{
    code_map::FuncBody,
    config::Config,
    const_pool::{ConstPool, ConstRef},
    func_builder::RelativeDepth,
    func_types::DedupFuncType,
};
use crate::{AsContext, AsContextMut, Func, FuncType};
use alloc::sync::Arc;
use spin::mutex::Mutex;
use wasmi_core::{Trap, UntypedValue};

/// The `wasmi` interpreter.
///
/// # Note
///
/// - The current `wasmi` engine implements a bytecode interpreter.
/// - This structure is intentionally cheap to copy.
///   Most of its API has a `&self` receiver, so can be shared easily.
#[derive(Debug, Clone)]
pub struct Engine {
    inner: Arc<Mutex<EngineInner>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}

impl Engine {
    /// Creates a new [`Engine`] with default configuration.
    ///
    /// # Note
    ///
    /// Users should ues [`Engine::default`] to construct a default [`Engine`].
    pub fn new(config: &Config) -> Self {
        Self {
            inner: Arc::new(Mutex::new(EngineInner::new(config))),
        }
    }

    /// Returns a shared reference to the [`Config`] of the [`Engine`].
    pub fn config(&self) -> Config {
        *self.inner.lock().config()
    }

    /// Allocates a new function type to the engine.
    pub(super) fn alloc_func_type(&self, func_type: FuncType) -> DedupFuncType {
        self.inner.lock().alloc_func_type(func_type)
    }

    /// Resolves a deduplicated function type into a [`FuncType`] entity.
    ///
    /// # Panics
    ///
    /// - If the deduplicated function type is not owned by the engine.
    /// - If the deduplicated function type cannot be resolved to its entity.
    pub(super) fn resolve_func_type<F, R>(&self, func_type: DedupFuncType, f: F) -> R
    where
        F: FnOnce(&FuncType) -> R,
    {
        self.inner.lock().resolve_func_type(func_type, f)
    }

    /// Resolves the [`FuncBody`] to the underlying `wasmi` bytecode instructions.
    ///
    /// # Note
    ///
    /// - This API is mainly intended for unit testing purposes and shall not be used
    ///   outside of this context. The function bodies are intended to be data private
    ///   to the `wasmi` interpreter.
    ///
    /// # Panics
    ///
    /// If the [`FuncBody`] is invalid for the [`Engine`].
    #[cfg(test)]
    pub(crate) fn resolve_inst(
        &self,
        func_body: FuncBody,
        index: usize,
    ) -> Option<ExecInstruction> {
        self.inner.lock().resolve_inst(func_body, index)
    }

    // /// Allocates the instructions of a Wasm function body to the [`Engine`].
    // ///
    // /// Returns a [`FuncBody`] reference to the allocated function body.
    // #[cfg(test)]
    // pub(super) fn alloc_func_body<I>(&self, insts: I, len_registers: u16) -> FuncBody
    // where
    //     I: IntoIterator<Item = ExecInstruction>,
    //     I::IntoIter: ExactSizeIterator,
    // {
    //     self.inner.lock().alloc_func_body(insts, len_registers)
    // }

    #[cfg(test)]
    pub(super) fn alloc_provider_slice<I>(&self, providers: I) -> ExecProviderSlice
    where
        I: IntoIterator<Item = ExecProvider>,
        I::IntoIter: ExactSizeIterator,
    {
        self.inner.lock().alloc_provider_slice(providers)
    }

    pub fn alloc_const<T>(&self, value: T) -> ConstRef
    where
        T: Into<UntypedValue>,
    {
        self.inner.lock().alloc_const(value)
    }

    pub fn compile<I>(&self, context: &CompileContext, insts: I) -> FuncBody
    where
        I: IntoIterator<Item = IrInstruction>,
    {
        self.inner.lock().compile(context, insts)
    }

    /// Executes the given [`Func`] using the given arguments `params` and stores the result into `results`.
    ///
    /// # Note
    ///
    /// This API assumes that the `params` and `results` are well typed and
    /// therefore won't perform type checks.
    /// Those checks are usually done at the [`Func::call`] API or when creating
    /// a new [`TypedFunc`] instance via [`Func::typed`].
    ///
    /// # Errors
    ///
    /// - If the given `func` is not a Wasm function, e.g. if it is a host function.
    /// - If the given arguments `params` do not match the expected parameters of `func`.
    /// - If the given `results` do not match the the length of the expected results of `func`.
    /// - When encountering a Wasm trap during the execution of `func`.
    ///
    /// [`TypedFunc`]: [`crate::TypedFunc`]
    pub(crate) fn execute_func<Params, Results>(
        &mut self,
        ctx: impl AsContextMut,
        func: Func,
        params: Params,
        results: Results,
    ) -> Result<<Results as CallResults>::Results, Trap>
    where
        Params: CallParams,
        Results: CallResults,
    {
        self.inner.lock().execute_func(ctx, func, params, results)
    }

    /// Returns a [`Display`] wrapper to pretty print the given function.
    ///
    /// # Note
    ///
    /// This functionality is intended for debugging purposes.
    ///
    /// [`Display`]: [`core::fmt::Display`]
    pub fn print_func(&self, ctx: impl AsContext, func: Func) {
        print!("{}", self.inner.lock().display_func(ctx.as_context(), func))
    }
}
