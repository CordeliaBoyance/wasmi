mod utils;

#[cfg(test)]
mod tests;

pub use self::utils::{ExecRegister, ExecRegisterSlice, Global, Offset, Target};
use super::{ConstRef, ExecProvider, ExecProviderSlice};
use crate::module::{FuncIdx, FuncTypeIdx};
use wasmi_core::TrapCode;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ExecuteTypes {}

impl InstructionTypes for ExecuteTypes {
    type Register = ExecRegister;
    type Provider = ExecProvider;
    type Immediate = ConstRef;
    type ProviderSlice = ExecProviderSlice;
    type RegisterSlice = ExecRegisterSlice;
    type Target = Target;
}

pub type ExecInstruction = Instruction<ExecuteTypes>;

/// Meta trait to customize [`Instruction`].
///
/// # Note
///
/// This is required since [`Instruction`] during construction of a
/// function and [`Instruction`] after finishing construction of a
/// function have a slightly different structure due to the different
/// needs they need to fulfil.
/// One needs to be easily adjustable and the other format needs to
/// be efficiently executable.
pub trait InstructionTypes {
    /// A plain register.
    type Register;
    /// A register or immediate value.
    type Provider;
    /// An immediate value.
    type Immediate;
    /// A slice of providers.
    type ProviderSlice;
    /// A slice of contiguous registers.
    type RegisterSlice;
    /// A branching target.
    type Target;
}

/// A `wasmi` instruction.
///
/// # Note
///
/// Internally `wasmi` uses register machine based instructions.
/// Upon module compilation and validation the stack machine based Wasm input
/// code is efficiently translated into this register machine based bytecode.
///
/// The reason we use register machine bytecode is that it executes
/// significantly faster than comparable stack machine based bytecode.
/// This is mostly due to the fact that fewer instructions are required
/// to represent the same behavior.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Instruction<T>
where
    T: InstructionTypes,
{
    /// Equivalent to the Wasm `br` instruction.
    Br {
        /// The target instruction to unconditionally branch to.
        target: T::Target,
    },
    /// Equivalent to the Wasm `br` instruction.
    ///
    /// # Note
    ///
    /// This `br` instruction also copies multiple values that its
    /// destination expects. This is important to efficiently support
    /// the Wasm `multi-value` proposal.
    BrMulti {
        /// The target instruction to unconditionally branch to.
        target: T::Target,
        /// The registers used as return values of the branched-to control block.
        results: T::RegisterSlice,
        /// The actual returned values for the branched-to control block.
        returned: T::ProviderSlice,
    },
    /// Branch iff `condition` evaluates to zero.
    ///
    /// # Note
    ///
    /// This instruction does not correspond to any Wasm instruction directly.
    ///
    /// Unlike with `BrNez` there are no `BrEqzSingle` and `BrEqzMulti`
    /// variants for copying single or multiple values while taking the
    /// conditional branch. This is simply because there is no need for them
    /// when compiling from WebAssembly.
    BrEqz {
        /// The target instruction to conditionally branch to.
        target: T::Target,
        /// The branching condition.
        condition: T::Register,
    },
    /// Used to represent the Wasm `br_if` instruction.
    ///
    /// # Note
    ///
    /// This instruction represents `br_if` only if the branch does not
    /// target the function body `block` and therefore does not return.
    BrNez {
        /// The target instruction to conditionally branch to.
        target: T::Target,
        /// The branching condition.
        condition: T::Register,
    },
    /// Used to represent the Wasm `br_if` instruction.
    ///
    /// # Note
    ///
    /// This instruction represents `br_if` only if the branch does not
    /// target the function body `block` and therefore does not return.
    ///
    /// This `br_nez` instruction also copies a single values that its
    /// destination expects. This is important to efficiently support
    /// Wasm blocks that return a result or Wasm `multi-value` loops that
    /// take a single parameter.
    BrNezSingle {
        /// The target instruction to conditionally branch to.
        target: T::Target,
        /// The branching condition.
        condition: T::Register,
        /// The register used as return value of the branched-to control block.
        result: T::Register,
        /// The actual returned value for the branched-to control block.
        returned: T::Provider,
    },
    /// Used to represent the Wasm `br_if` instruction.
    ///
    /// # Note
    ///
    /// This instruction represents `br_if` only if the branch does not
    /// target the function body `block` and therefore does not return.
    ///
    /// This `br` instruction also copies multiple values that its
    /// destination expects. This is important to efficiently support
    /// the Wasm `multi-value` proposal.
    BrNezMulti {
        /// The target instruction to conditionally branch to.
        target: T::Target,
        /// The branching condition.
        condition: T::Register,
        /// The registers used as return values of the branched-to control block.
        results: T::RegisterSlice,
        /// The actual returned values for the branched-to control block.
        returned: T::ProviderSlice,
    },
    /// Used to represent the Wasm `br_if` instruction.
    ///
    /// # Note
    ///
    /// This instruction represents `br_if` only if the branch targets
    /// the function body `block` and therefore returns to the caller.
    ReturnNez {
        /// The registers used as return values of the function.
        results: T::ProviderSlice,
        /// The branching condition.
        condition: T::Register,
    },
    /// Equivalent to the Wasm `br_table` instruction.
    ///
    /// # Note
    ///
    /// This instruction must be followed by `len_targets` instructions
    /// that are either [`Instruction::Br`] or [`Instruction::Return`].
    BrTable {
        /// The case for the branch.
        case: T::Register,
        /// The amount of targets of this branching table including the default.
        len_targets: usize,
    },
    /// Equivalent to the Wasm `unreachable` instruction.
    ///
    /// # Note
    ///
    /// This allows to represent Wasm's `unreachable` instruction but also
    /// allows to represent other invalid instructions.
    /// This is especially useful for constant folding fallible instructions
    /// such as `i32.div 42 0` which can be evaluated to a trap at compilation
    /// time. Instead of trapping during compilation of such code `wasmi` simply
    /// emits the proper trap instead of the `i32.div` instruction.
    Trap { trap_code: TrapCode },
    /// Equivalent to the Wasm `return` instruction.
    Return {
        /// The registers used as return values of the function.
        results: T::ProviderSlice,
    },
    /// Equivalent to the Wasm `call` instruction.
    Call {
        /// The function index of the called function.
        func_idx: FuncIdx,
        /// The registers used as result values of the call.
        ///
        /// # Note
        ///
        /// We can use the more efficient [`ExecRegisterSlice`]
        /// here since we can guarantee that result register indices are
        /// always contigous.
        /// Since we are supporting the `multi-value` Wasm proposal
        /// we are required to represent more than one result value.
        results: T::RegisterSlice,
        /// The parameters of the function call.
        params: T::ProviderSlice,
    },
    /// Equivalent to the Wasm `call_indirect` instruction.
    CallIndirect {
        /// The index of the function type of the indirectly called function.
        func_type_idx: FuncTypeIdx,
        /// The registers used as result values of the call.
        ///
        /// # Note
        ///
        /// We can use the more efficient [`ExecRegisterSlice`]
        /// here since we can guarantee that result register indices are
        /// always contigous.
        /// Since we are supporting the `multi-value` Wasm proposal
        /// we are required to represent more than one result value.
        results: T::RegisterSlice,
        /// The index into the table used for the indirect function call.
        ///
        /// TODO 1: might `T::Register` be more useful here?
        /// TODO 2: we might be able to embed this into the `params` field
        ///         to save some data structure space.
        index: T::Provider,
        /// The parameters of the indirect function call.
        params: T::ProviderSlice,
    },
    /// Copies the `input` into the `result`.
    ///
    /// # Note
    ///
    /// This instruction does not correspond to any Wasm instruction directly.
    /// However, due to the way we translate Wasm bytecode into `wasmi` bytecode
    /// we sometimes are required to insert a few copy instructions.
    /// For example with those copy instructions we can manipulate the
    /// emulation stack in cases where the stack becomes polymorphic.
    Copy {
        /// The register where the copy will be stored.
        result: T::Register,
        /// The input register to copy.
        input: T::Register,
    },
    /// Copies the `input` into the `result`.
    ///
    /// # Note
    ///
    /// This instruction does not correspond to any Wasm instruction directly.
    /// However, due to the way we translate Wasm bytecode into `wasmi` bytecode
    /// we sometimes are required to insert a few copy instructions.
    /// For example with those copy instructions we can manipulate the
    /// emulation stack in cases where the stack becomes polymorphic.
    CopyImm {
        /// The register where the copy will be stored.
        result: T::Register,
        /// The input immediate value to copy.
        input: T::Immediate,
    },
    /// Copies many values from `inputs` into `results`.
    ///
    /// # Note
    ///
    /// This instruction is a more efficient version of the `Copy` instruction
    /// in cases where many values need to be copied around. This can for
    /// example happen with the Wasm `multi-value` proposal in certain
    /// scenarios.
    ///
    /// This instruction does not correspond to any Wasm instruction directly.
    /// However, due to the way we translate Wasm bytecode into `wasmi` bytecode
    /// we sometimes are required to insert a few copy instructions.
    /// For example with those copy instructions we can manipulate the
    /// emulation stack in cases where the stack becomes polymorphic.
    CopyMany {
        /// The registers where the copies will be stored.
        results: T::RegisterSlice,
        /// The input registers or immediate values to copy.
        inputs: T::ProviderSlice,
    },
    /// Equivalent to the Wasm `select` instruction.
    Select {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Decides whether `if_true` or `if_false` will be stored into `result`.
        condition: T::Register,
        /// Stored into `result` if `condition` evaluates to `1` (true).
        if_true: T::Provider,
        /// Stored into `result` if `condition` evaluates to `0` (false).
        if_false: T::Provider,
    },
    /// Equivalent to the Wasm `global.get` instruction.
    GlobalGet {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The index of the global variable to retrieve the value.
        global: Global,
    },
    /// Equivalent to the Wasm `global.set` instruction.
    GlobalSet {
        /// The index of the global variable to set the value.
        global: Global,
        /// The new value of the global variable.
        value: T::Provider,
    },
    /// Equivalent to the Wasm `i32.load` instruction.
    I32Load {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i64.load` instruction.
    I64Load {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `f32.load` instruction.
    F32Load {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `f64.load` instruction.
    F64Load {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i32.load8_s` instruction.
    I32Load8S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i32.load8_u` instruction.
    I32Load8U {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i32.load16_s` instruction.
    I32Load16S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i32.load16_u` instruction.
    I32Load16U {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i64.load8_s` instruction.
    I64Load8S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i64.load8_u` instruction.
    I64Load8U {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i64.load16_s` instruction.
    I64Load16S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i64.load16_u` instruction.
    I64Load16U {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i64.load32_s` instruction.
    I64Load32S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i64.load32_u` instruction.
    I64Load32U {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
    },
    /// Equivalent to the Wasm `i32.store` instruction.
    I32Store {
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
        /// The value to be stored into linear memory.
        value: T::Provider,
    },
    /// Equivalent to the Wasm `i64.store` instruction.
    I64Store {
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
        /// The value to be stored in linear memory.
        value: T::Provider,
    },
    /// Equivalent to the Wasm `f32.store` instruction.
    F32Store {
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
        /// The value to be stored in linear memory.
        value: T::Provider,
    },
    /// Equivalent to the Wasm `f64.store` instruction.
    F64Store {
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
        /// The value to be stored in linear memory.
        value: T::Provider,
    },
    /// Equivalent to the Wasm `i32.store8` instruction.
    I32Store8 {
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
        /// The value to be stored in linear memory.
        value: T::Provider,
    },
    /// Equivalent to the Wasm `i32.store16` instruction.
    I32Store16 {
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
        /// The value to be stored in linear memory.
        value: T::Provider,
    },
    /// Equivalent to the Wasm `i64.store8` instruction.
    I64Store8 {
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
        /// The value to be stored in linear memory.
        value: T::Provider,
    },
    /// Equivalent to the Wasm `i64.store16` instruction.
    I64Store16 {
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
        /// The value to be stored in linear memory.
        value: T::Provider,
    },
    /// Equivalent to the Wasm `i64.store32` instruction.
    I64Store32 {
        /// The base pointer to the linear memory region.
        ptr: T::Register,
        /// The offset added to the base pointer for the instruction.
        offset: Offset,
        /// The value to be stored in linear memory.
        value: T::Provider,
    },
    /// Equivalent to the Wasm `memory.size` instruction.
    MemorySize {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
    },
    /// Equivalent to the Wasm `memory.grow` instruction.
    MemoryGrow {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The amount of additional linear memory pages.
        amount: T::Provider,
    },
    /// Equivalent to the Wasm `i32.eq` instruction.
    I32Eq {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.ne` instruction.
    I32Ne {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.lt_s` instruction.
    I32LtS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.lt_u` instruction.
    I32LtU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.gt_s` instruction.
    I32GtS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.gt_u` instruction.
    I32GtU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.le_s` instruction.
    I32LeS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.le_u` instruction.
    I32LeU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.ge_s` instruction.
    I32GeS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.ge_u` instruction.
    I32GeU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.eq` instruction.
    I64Eq {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.ne` instruction.
    I64Ne {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.lt_s` instruction.
    I64LtS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.lt_u` instruction.
    I64LtU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.gt_s` instruction.
    I64GtS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.gt_u` instruction.
    I64GtU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.le_s` instruction.
    I64LeS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.le_u` instruction.
    I64LeU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.ge_s` instruction.
    I64GeS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.ge_u` instruction.
    I64GeU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.eq` instruction.
    F32Eq {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.ne` instruction.
    F32Ne {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.lt` instruction.
    F32Lt {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.gt` instruction.
    F32Gt {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.le` instruction.
    F32Le {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.ge` instruction.
    F32Ge {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.eq` instruction.
    F64Eq {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.ne` instruction.
    F64Ne {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.lt` instruction.
    F64Lt {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.gt` instruction.
    F64Gt {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.le` instruction.
    F64Le {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.ge` instruction.
    F64Ge {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.clz` instruction.
    I32Clz {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.ctz` instruction.
    I32Ctz {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.popcnt` instruction.
    I32Popcnt {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.add` instruction.
    I32Add {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.sub` instruction.
    I32Sub {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.mul` instruction.
    I32Mul {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.div_s` instruction.
    I32DivS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.div_u` instruction.
    I32DivU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.rem_s` instruction.
    I32RemS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.rem_u` instruction.
    I32RemU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.and` instruction.
    I32And {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.or` instruction.
    I32Or {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.xor` instruction.
    I32Xor {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.shl` instruction.
    I32Shl {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.shr_s` instruction.
    I32ShrS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.shr_u` instruction.
    I32ShrU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.rotl` instruction.
    I32Rotl {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.rotr` instruction.
    I32Rotr {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.clz` instruction.
    I64Clz {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.ctz` instruction.
    I64Ctz {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.popcnt` instruction.
    I64Popcnt {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.add` instruction.
    I64Add {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.sub` instruction.
    I64Sub {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.mul` instruction.
    I64Mul {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.div_s` instruction.
    I64DivS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.div_u` instruction.
    I64DivU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.rem_s` instruction.
    I64RemS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.rem_u` instruction.
    I64RemU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.and` instruction.
    I64And {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.or` instruction.
    I64Or {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.xor` instruction.
    I64Xor {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.shl` instruction.
    I64Shl {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.shr_s` instruction.
    I64ShrS {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.shr_u` instruction.
    I64ShrU {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.rotl` instruction.
    I64Rotl {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i64.rotr` instruction.
    I64Rotr {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.abs` instruction.
    F32Abs {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.neg` instruction.
    F32Neg {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.ceil` instruction.
    F32Ceil {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.floor` instruction.
    F32Floor {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.trunc` instruction.
    F32Trunc {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.nearest` instruction.
    F32Nearest {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.sqrt` instruction.
    F32Sqrt {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.add` instruction.
    F32Add {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.sub` instruction.
    F32Sub {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.mul` instruction.
    F32Mul {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.div` instruction.
    F32Div {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.min` instruction.
    F32Min {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.max` instruction.
    F32Max {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f32.copysign` instruction.
    F32Copysign {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.abs` instruction.
    F64Abs {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.neg` instruction.
    F64Neg {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.ceil` instruction.
    F64Ceil {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.floor` instruction.
    F64Floor {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.trunc` instruction.
    F64Trunc {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.nearest` instruction.
    F64Nearest {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.sqrt` instruction.
    F64Sqrt {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.add` instruction.
    F64Add {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.sub` instruction.
    F64Sub {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.mul` instruction.
    F64Mul {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.div` instruction.
    F64Div {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.min` instruction.
    F64Min {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.max` instruction.
    F64Max {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `f64.copysign` instruction.
    F64Copysign {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// The left-hand side argument of the instruction.
        lhs: T::Register,
        /// The right-hand side argument of the instruction.
        rhs: T::Provider,
    },
    /// Equivalent to the Wasm `i32.wrap_i64` instruction.
    I32WrapI64 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.trunc_f32_s` instruction.
    I32TruncSF32 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.trunc_f32_u` instruction.
    I32TruncUF32 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.trunc_f64_s` instruction.
    I32TruncSF64 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.trunc_f64_u` instruction.
    I32TruncUF64 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.extend_i32_s` instruction.
    I64ExtendSI32 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.extend_i32_u` instruction.
    I64ExtendUI32 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.trunc_f32_s` instruction.
    I64TruncSF32 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.trunc_f32_u` instruction.
    I64TruncUF32 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.trunc_f64_s` instruction.
    I64TruncSF64 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.trunc_f64_u` instruction.
    I64TruncUF64 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.convert_i32_s` instruction.
    F32ConvertSI32 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.convert_i32_u` instruction.
    F32ConvertUI32 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.convert_i64_s` instruction.
    F32ConvertSI64 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.convert_i64_u` instruction.
    F32ConvertUI64 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f32.demote_f64` instruction.
    F32DemoteF64 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.convert_i32_s` instruction.
    F64ConvertSI32 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.convert_i32_u` instruction.
    F64ConvertUI32 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.convert_i64_s` instruction.
    F64ConvertSI64 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.convert_i64_u` instruction.
    F64ConvertUI64 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `f64.promote_f32` instruction.
    F64PromoteF32 {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input to the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.extend8_s` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the [`sign-extension`
    /// Wasm proposal].
    ///
    /// [`sign-extension` Wasm proposal]:
    /// https://github.com/WebAssembly/sign-extension-ops
    I32Extend8S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.extend16_s` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the [`sign-extension`
    /// Wasm proposal].
    ///
    /// [`sign-extension` Wasm proposal]:
    /// https://github.com/WebAssembly/sign-extension-ops
    I32Extend16S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.extend8_s` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the [`sign-extension`
    /// Wasm proposal].
    ///
    /// [`sign-extension` Wasm proposal]:
    /// https://github.com/WebAssembly/sign-extension-ops
    I64Extend8S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.extend16_s` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the [`sign-extension`
    /// Wasm proposal].
    ///
    /// [`sign-extension` Wasm proposal]:
    /// https://github.com/WebAssembly/sign-extension-ops
    I64Extend16S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.extend32_s` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the [`sign-extension`
    /// Wasm proposal].
    ///
    /// [`sign-extension` Wasm proposal]:
    /// https://github.com/WebAssembly/sign-extension-ops
    I64Extend32S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.trunc_sat_f32_s` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the
    /// [`saturating-float-to-int` Wasm proposal].
    ///
    /// [`saturating-float-to-int` Wasm proposal]:
    /// https://github.com/WebAssembly/nontrapping-float-to-int-conversions
    I32TruncSatF32S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.trunc_sat_f32_u` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the
    /// [`saturating-float-to-int` Wasm proposal].
    ///
    /// [`saturating-float-to-int` Wasm proposal]:
    /// https://github.com/WebAssembly/nontrapping-float-to-int-conversions
    I32TruncSatF32U {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.trunc_sat_f64_s` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the
    /// [`saturating-float-to-int` Wasm proposal].
    ///
    /// [`saturating-float-to-int` Wasm proposal]:
    /// https://github.com/WebAssembly/nontrapping-float-to-int-conversions
    I32TruncSatF64S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i32.trunc_sat_f64_u` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the
    /// [`saturating-float-to-int` Wasm proposal].
    ///
    /// [`saturating-float-to-int` Wasm proposal]:
    /// https://github.com/WebAssembly/nontrapping-float-to-int-conversions
    I32TruncSatF64U {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.trunc_sat_f32_s` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the
    /// [`saturating-float-to-int` Wasm proposal].
    ///
    /// [`saturating-float-to-int` Wasm proposal]:
    /// https://github.com/WebAssembly/nontrapping-float-to-int-conversions
    I64TruncSatF32S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.trunc_sat_f32_u` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the
    /// [`saturating-float-to-int` Wasm proposal].
    ///
    /// [`saturating-float-to-int` Wasm proposal]:
    /// https://github.com/WebAssembly/nontrapping-float-to-int-conversions
    I64TruncSatF32U {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.trunc_sat_f64_s` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the
    /// [`saturating-float-to-int` Wasm proposal].
    ///
    /// [`saturating-float-to-int` Wasm proposal]:
    /// https://github.com/WebAssembly/nontrapping-float-to-int-conversions
    I64TruncSatF64S {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
    /// Equivalent to the Wasm `i64.trunc_sat_f64_u` instruction.
    ///
    /// # Note
    ///
    /// This instruction is part of the
    /// [`saturating-float-to-int` Wasm proposal].
    ///
    /// [`saturating-float-to-int` Wasm proposal]:
    /// https://github.com/WebAssembly/nontrapping-float-to-int-conversions
    I64TruncSatF64U {
        /// Stores the result of the instruction evaluation.
        result: T::Register,
        /// Stores the input for the instruction evaluation.
        input: T::Register,
    },
}
