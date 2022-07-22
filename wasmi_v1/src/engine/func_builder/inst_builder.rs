//! Abstractions to build up instructions forming Wasm function bodies.

use super::{
    labels::{LabelRef, LabelRegistry},
    providers::Providers,
    CompileContext,
    Engine,
    FuncBody,
    IrInstruction,
    IrProvider,
    IrProviderSlice,
    IrRegister,
    IrRegisterSlice,
    ProviderSliceArena,
};
use crate::arena::Index;
use alloc::vec::Vec;

/// A reference to a partially constructed instruction.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Instr(u32);

impl Instr {
    /// An invalid instruction.
    ///
    /// # Note
    ///
    /// This can be used to represent invalid instructions without introducing
    /// overhead for example by wrapping an instruction inside an [`Option`].
    pub const INVALID: Self = Self(u32::MAX);

    /// Returns the inner `u32` value.
    pub fn into_inner(self) -> u32 {
        self.0
    }

    /// Creates an [`Instr`] from a raw `u32` value.
    pub fn from_inner(index: u32) -> Self {
        Self(index)
    }
}

impl Index for Instr {
    fn into_usize(self) -> usize {
        self.0 as usize
    }

    fn from_usize(index: usize) -> Self {
        let index = index.try_into().unwrap_or_else(|error| {
            panic!(
                "encountered invalid index of {} for `Inst`: {}",
                index, error
            )
        });
        assert_ne!(index, u32::MAX, "tried to create an invalid Instr");
        Self(index)
    }
}

/// The relative depth of a Wasm branching target.
#[derive(Debug, Copy, Clone)]
pub struct RelativeDepth(u32);

impl RelativeDepth {
    /// Returns the relative depth as `u32`.
    pub fn into_u32(self) -> u32 {
        self.0
    }

    /// Creates a relative depth from the given `u32` value.
    pub fn from_u32(relative_depth: u32) -> Self {
        Self(relative_depth)
    }
}

/// An instruction builder.
///
/// Allows to incrementally and efficiently build up the instructions
/// of a Wasm function body.
/// Can be reused to build multiple functions consecutively.
#[derive(Debug, Default)]
pub struct InstructionsBuilder {
    /// The instructions of the partially constructed function body.
    insts: Vec<IrInstruction>,
    /// All labels and their uses.
    labels: LabelRegistry,
}

impl InstructionsBuilder {
    /// Returns the current instruction pointer as index.
    pub fn current_pc(&self) -> Instr {
        Instr::from_usize(self.insts.len())
    }

    /// Creates a new unresolved label and returns an index to it.
    pub fn new_label(&mut self) -> LabelRef {
        self.labels.new_label()
    }

    /// Pins the `label` to the next pushed instruction.
    ///
    /// # Panics
    ///
    /// If the `label` has already been pinned.
    pub fn pin_label(&mut self, label: LabelRef) {
        let instr = self.current_pc();
        self.labels
            .pin_label(label, instr)
            .unwrap_or_else(|error| panic!("failed to pin label: {error}"));
    }

    /// Pins a `label` to the next pushed instruction if unpinned.
    pub fn try_pin_label(&mut self, label: LabelRef) {
        let instr = self.current_pc();
        self.labels.try_pin_label(label, instr)
    }

    /// Pushes the internal instruction bytecode to the [`InstructionsBuilder`].
    ///
    /// Returns an [`Instr`] to refer to the pushed instruction.
    pub fn push_inst(&mut self, inst: IrInstruction) -> Instr {
        let idx = self.current_pc();
        self.insts.push(inst);
        idx
    }

    /// Pushes a `copy` instruction to the [`InstructionsBuilder`].
    ///
    /// Does not push a `copy` instruction if the `result` and `input`
    /// registers are equal and thereby the `copy` would be a no-op. In
    /// this case this function returns `None`.
    ///
    /// Otherwise this function returns a reference to the created `copy`
    /// instruction.
    pub fn push_copy_instr(&mut self, result: IrRegister, input: IrProvider) -> Option<Instr> {
        if let IrProvider::Register(input) = input {
            if result == input {
                // Both `result` and `input` registers are the same
                // so the `copy` instruction would be a no-op.
                // Therefore we can avoid serializing it.
                return None;
            }
        }
        let instr = match input {
            IrProvider::Register(input) => self.push_inst(IrInstruction::Copy { result, input }),
            IrProvider::Immediate(input) => {
                self.push_inst(IrInstruction::CopyImm { result, input })
            }
        };
        Some(instr)
    }

    /// Pushes a `copy_many` instruction to the [`InstructionsBuilder`].
    ///
    /// This filters out any non-true copies at the `results` start or end.
    pub fn push_copy_many_instr<'a>(
        &mut self,
        arena: &mut ProviderSliceArena,
        results: IrRegisterSlice,
        inputs: &'a [IrProvider],
    ) -> Option<Instr> {
        match TrueCopies::analyze(arena, results, inputs) {
            TrueCopies::None => None,
            TrueCopies::Single { result, input } => self.push_copy_instr(result, input),
            TrueCopies::Many { results, inputs } => {
                Some(self.push_inst(IrInstruction::CopyMany { results, inputs }))
            }
        }
    }

    /// Pushes a `br` instruction to the [`InstructionsBuilder`].
    ///
    /// Depending on the actual amount of true copies this pushes one of the
    /// following sequences of instructions to the [`InstructionsBuilder`].
    ///
    /// 1. **No true copies:** `br` instruction.
    /// 2. **Single true copy:** `copy` + `br` instruction
    /// 3. **Many true copies:** `br_multi` instruction
    pub fn push_br(
        &mut self,
        arena: &mut ProviderSliceArena,
        target: LabelRef,
        results: IrRegisterSlice,
        inputs: IrProviderSlice,
    ) -> Instr {
        match TrueCopies::analyze_slice(arena, results, inputs) {
            TrueCopies::None => self.push_inst(IrInstruction::Br { target }),
            TrueCopies::Single { result, input } => match input {
                IrProvider::Register(returned) => self.push_inst(IrInstruction::BrCopy {
                    target,
                    result,
                    returned,
                }),
                IrProvider::Immediate(returned) => self.push_inst(IrInstruction::BrCopyImm {
                    target,
                    result,
                    returned,
                }),
            },
            TrueCopies::Many { results, inputs } => self.push_inst(IrInstruction::BrCopyMulti {
                target,
                results,
                returned: inputs,
            }),
        }
    }

    /// Pushes a `br_eqz` instruction to the [`InstructionsBuilder`].
    ///
    /// Depending on the actual amount of true copies this pushes one of the
    /// following sequences of instructions to the [`InstructionsBuilder`].
    ///
    /// 1. **No true copies:** `br_nez` instruction.
    /// 2. **Single true copy:** `copy` + `br_nez` instruction
    /// 3. **Many true copies:** `br_nez_multi` instruction
    pub fn push_br_nez(
        &mut self,
        arena: &mut ProviderSliceArena,
        target: LabelRef,
        condition: IrRegister,
        results: IrRegisterSlice,
        inputs: IrProviderSlice,
    ) -> Instr {
        match TrueCopies::analyze_slice(arena, results, inputs) {
            TrueCopies::None => self.push_inst(IrInstruction::BrNez { target, condition }),
            TrueCopies::Single { result, input } => self.push_inst(IrInstruction::BrNezSingle {
                target,
                condition,
                result,
                returned: input,
            }),
            TrueCopies::Many { results, inputs } => self.push_inst(IrInstruction::BrNezMulti {
                target,
                condition,
                results,
                returned: inputs,
            }),
        }
    }

    /// Peeks the last instruction pushed to the instruction builder if any.
    pub fn peek_mut(&mut self) -> Option<&mut IrInstruction> {
        self.insts.last_mut()
    }

    /// Finishes construction of the function body instructions.
    ///
    /// # Note
    ///
    /// This feeds the built-up instructions of the function body
    /// into the [`Engine`] so that the [`Engine`] is
    /// aware of the Wasm function existance. Returns a `FuncBody`
    /// reference that allows to retrieve the instructions.
    #[must_use]
    pub fn finish(
        &mut self,
        engine: &Engine,
        reg_slices: &ProviderSliceArena,
        providers: &Providers,
    ) -> FuncBody {
        let context = CompileContext {
            provider_slices: reg_slices,
            providers,
            labels: &self.labels,
        };
        engine.compile(&context, self.insts.drain(..))
    }
}

/// The result of a `CopyMany` optimization.
#[derive(Debug, Copy, Clone)]
pub enum TrueCopies {
    /// There are no true copies.
    None,
    /// There is only a single true copy.
    Single {
        /// The single result of the true copy.
        result: IrRegister,
        /// The single input of the true copy.
        input: IrProvider,
    },
    /// There are many true copies.
    ///
    /// This case might also include non-true copies
    /// since `IrRegisterSlice` can only represent
    /// contiguous registers.
    Many {
        /// The results of the copies.
        results: IrRegisterSlice,
        /// The inputs of the copies.
        inputs: IrProviderSlice,
    },
}

impl TrueCopies {
    fn true_copies_iter(
        results: IrRegisterSlice,
        inputs: &[IrProvider],
    ) -> impl Iterator<Item = (usize, (IrRegister, IrProvider))> + '_ {
        // Instead of taking the raw number of inputs and results
        // we take the number of actual true copies filtering out
        // any no-op copies.
        // E.g. `(x0, x1) <- (x1, x1)` has only one true copy `x0 <- x1`
        // and the copy `x1 <- x1` is superflous.
        results
            .iter()
            .zip(inputs.iter().copied())
            .enumerate()
            .filter(|(_nth, (result, input))| {
                if let IrProvider::Register(input) = input {
                    return result != input;
                }
                true
            })
    }

    fn count_true_copies(results: IrRegisterSlice, inputs: &[IrProvider]) -> usize {
        Self::true_copies_iter(results, inputs).count()
    }

    /// Analyzes the given `results` and `inputs` with respect to true copies.
    ///
    /// True copies are when result and input registers are not the same.
    /// This filters out any non-true copies at the start and end of the
    /// register slices.
    /// The [`TrueCopies::Many`] case might include non-true copies due to the
    /// way [`IrRegisterSlice`] can only represent contiguous registers.
    ///
    /// # Note
    ///
    /// This function exists to improve testability of the procedure.
    pub fn analyze_slice(
        arena: &mut ProviderSliceArena,
        results: IrRegisterSlice,
        inputs: IrProviderSlice,
    ) -> Self {
        let slice = arena.resolve(inputs);
        let len_results = results.len() as usize;
        let len_inputs = slice.len();
        debug_assert_eq!(len_results, len_inputs);
        match Self::count_true_copies(results, slice) {
            0 => {
                // Case: copy of no elements
                //
                // We can simply bail out and push no instruction in this case.
                Self::None
            }
            1 => {
                // Case: copy of one one element
                //
                // We can use the more efficient `Copy` instruction instead.
                let (_, (result, input)) = Self::true_copies_iter(results, slice)
                    .next()
                    .expect("non-empty true copies");
                Self::Single { result, input }
            }
            n if n == len_results => {
                // Case: copy as many elements as have been given
                Self::Many { results, inputs }
            }
            _ => {
                // Case: copy of many elements
                //
                // We actually have to serialize the `CopyMany` instruction.
                //
                // TODO: we could further filter out no-op copies in this case
                //       if we detect that all true copies are neighbouring
                //       each other. For example `(x0, x1, x2, x3) <- (x0, x2, x3, x3)`
                //       has two true copies `x1 <- x2` and `x2 <- x3` and they
                //       are neighbouring each other, so we can filter out the
                //       other no-op copies.
                //       However, for (`x0, x1, x2, x3) <- (x1, x1, x2, x2)` we
                //       cannot do this since the two true copies `x0 <- x1`
                //       and `x3 <- x2` are not neighbouring each other.
                let (first_index, last_index) = {
                    let mut copies = Self::true_copies_iter(results, slice);
                    let (first_index, _) = copies.next().expect("non-empty true copies");
                    let (last_index, _) = copies.last().expect("non-empty true copies");
                    (first_index, last_index + 1)
                };
                let len = last_index - first_index;
                let inputs = inputs.skip(first_index as u32).take(len as u32);
                let _ = slice;
                let results = results
                    .sub_slice(first_index..last_index)
                    .expect("indices in bounds");
                Self::Many { results, inputs }
            }
        }
    }

    /// Analyzes the given `results` and `inputs` with respect to true copies.
    ///
    /// True copies are when result and input registers are not the same.
    /// This filters out any non-true copies at the start and end of the
    /// register slices.
    /// The [`TrueCopies::Many`] case might include non-true copies due to the
    /// way [`IrRegisterSlice`] can only represent contiguous registers.
    ///
    /// # Note
    ///
    /// This function exists to improve testability of the procedure.
    pub fn analyze(
        arena: &mut ProviderSliceArena,
        results: IrRegisterSlice,
        inputs: &[IrProvider],
    ) -> Self {
        let len_results = results.len() as usize;
        let len_inputs = inputs.len();
        debug_assert_eq!(len_results, len_inputs);
        match Self::count_true_copies(results, inputs) {
            0 => {
                // Case: copy of no elements
                //
                // We can simply bail out and push no instruction in this case.
                Self::None
            }
            1 => {
                // Case: copy of one one element
                //
                // We can use the more efficient `Copy` instruction instead.
                let (_, (result, input)) = Self::true_copies_iter(results, inputs)
                    .next()
                    .expect("non-empty true copies");
                Self::Single { result, input }
            }
            _ => {
                // Case: copy of many elements
                //
                // We actually have to serialize the `CopyMany` instruction.
                //
                // TODO: we could further filter out no-op copies in this case
                //       if we detect that all true copies are neighbouring
                //       each other. For example `(x0, x1, x2, x3) <- (x0, x2, x3, x3)`
                //       has two true copies `x1 <- x2` and `x2 <- x3` and they
                //       are neighbouring each other, so we can filter out the
                //       other no-op copies.
                //       However, for (`x0, x1, x2, x3) <- (x1, x1, x2, x2)` we
                //       cannot do this since the two true copies `x0 <- x1`
                //       and `x3 <- x2` are not neighbouring each other.
                let (first_index, last_index) = {
                    let mut copies = Self::true_copies_iter(results, inputs);
                    let (first_index, _) = copies.next().expect("non-empty true copies");
                    let (last_index, _) = copies.last().expect("non-empty true copies");
                    (first_index, last_index + 1)
                };
                let len = last_index - first_index;
                let inputs = inputs.iter().copied().skip(first_index).take(len);
                let results = results
                    .sub_slice(first_index..last_index)
                    .expect("indices in bounds");
                let inputs = arena.alloc(inputs);
                Self::Many { results, inputs }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_providers_eq(arena: &ProviderSliceArena, lhs: IrProviderSlice, rhs: IrProviderSlice) {
        let lhs = arena.resolve(lhs);
        let rhs = arena.resolve(rhs);
        assert_eq!(lhs, rhs)
    }

    fn assert_true_copies_eq(arena: &ProviderSliceArena, lhs: TrueCopies, rhs: TrueCopies) {
        match (lhs, rhs) {
            (TrueCopies::None, TrueCopies::None) => (),
            (
                TrueCopies::Single {
                    result: lhs_result,
                    input: lhs_input,
                },
                TrueCopies::Single {
                    result: rhs_result,
                    input: rhs_input,
                },
            ) => {
                assert_eq!(lhs_result, rhs_result);
                assert_eq!(lhs_input, rhs_input);
            }
            (
                TrueCopies::Many {
                    results: lhs_results,
                    inputs: lhs_inputs,
                },
                TrueCopies::Many {
                    results: rhs_results,
                    inputs: rhs_inputs,
                },
            ) => {
                assert_eq!(lhs_results, rhs_results);
                assert_providers_eq(arena, lhs_inputs, rhs_inputs);
            }
            (lhs, rhs) => panic!("lhs != rhs\nlhs = {lhs:?}\nrhs = {rhs:?}"),
        }
    }

    fn register_slice(start: usize, len: u16) -> IrRegisterSlice {
        IrRegisterSlice::new(IrRegister::Dynamic(start), len)
    }

    fn provider_reg(index: usize) -> IrProvider {
        IrProvider::Register(IrRegister::Dynamic(index))
    }

    #[test]
    fn test_analyze_true_copies() {
        let mut arena = ProviderSliceArena::default();

        // Case: empty slices
        {
            let results = IrRegisterSlice::empty();
            let inputs = &[];
            let actual = TrueCopies::analyze(&mut arena, results, inputs);
            let expected = TrueCopies::None;
            assert_true_copies_eq(&arena, actual, expected);
        }

        // Case: no actual copies
        //
        // (x0, x1) <- (x0, x1)
        {
            let results = register_slice(0, 2);
            let inputs = &[provider_reg(0), provider_reg(1)];
            let actual = TrueCopies::analyze(&mut arena, results, inputs);
            let expected = TrueCopies::None;
            assert_true_copies_eq(&arena, actual, expected);
        }

        // Case: single actual copy
        //
        // x0 <- x1
        {
            let results = register_slice(0, 1);
            let inputs = &[provider_reg(1)];
            let actual = TrueCopies::analyze(&mut arena, results, inputs);
            let expected = TrueCopies::Single {
                result: IrRegister::Dynamic(0),
                input: IrProvider::Register(IrRegister::Dynamic(1)),
            };
            assert_true_copies_eq(&arena, actual, expected);
        }

        // Case: single true copy at start
        //
        // (x0, x1) <- (x1, x1)
        // => x0 <- x1
        {
            let results = register_slice(0, 2);
            let inputs = &[provider_reg(1), provider_reg(1)];
            let actual = TrueCopies::analyze(&mut arena, results, inputs);
            let expected = TrueCopies::Single {
                result: IrRegister::Dynamic(0),
                input: IrProvider::Register(IrRegister::Dynamic(1)),
            };
            assert_true_copies_eq(&arena, actual, expected);
        }

        // Case: single true copy at end
        //
        // (x0, x1) <- (x0, x0)
        // => x1 <- x0
        {
            let results = register_slice(0, 2);
            let inputs = &[provider_reg(0), provider_reg(0)];
            let actual = TrueCopies::analyze(&mut arena, results, inputs);
            let expected = TrueCopies::Single {
                result: IrRegister::Dynamic(1),
                input: IrProvider::Register(IrRegister::Dynamic(0)),
            };
            assert_true_copies_eq(&arena, actual, expected);
        }

        // Case: single true copy in the middle
        //
        // (x0, x1, x2) <- (x0, x3, x2)
        // => x1 <- x3
        {
            let results = register_slice(0, 3);
            let inputs = &[provider_reg(0), provider_reg(3), provider_reg(2)];
            let actual = TrueCopies::analyze(&mut arena, results, inputs);
            let expected = TrueCopies::Single {
                result: IrRegister::Dynamic(1),
                input: IrProvider::Register(IrRegister::Dynamic(3)),
            };
            assert_true_copies_eq(&arena, actual, expected);
        }

        // Case: many true copies
        //
        // (x0, x1) <- (x2, x2)
        {
            let results = register_slice(0, 2);
            let inputs = &[provider_reg(2), provider_reg(2)];
            let actual = TrueCopies::analyze(&mut arena, results, inputs);
            let expected = TrueCopies::Many {
                results,
                inputs: arena.alloc(inputs.iter().copied()),
            };
            assert_true_copies_eq(&arena, actual, expected);
        }

        // Case: many true copies at the end
        //
        // (x0, x1, x2) <- (x0, x3, x3)
        // => (x1, x2) <- (x3, x3)
        {
            let results = register_slice(0, 3);
            let inputs = &[provider_reg(0), provider_reg(3), provider_reg(3)];
            let actual = TrueCopies::analyze(&mut arena, results, inputs);
            let expected = TrueCopies::Many {
                results: register_slice(1, 2),
                inputs: arena.alloc([provider_reg(3), provider_reg(3)]),
            };
            assert_true_copies_eq(&arena, actual, expected);
        }

        // Case: many true copies at the start
        //
        // (x0, x1, x2) <- (x2, x2, x2)
        // => (x0, x1) <- (x2, x2)
        {
            let results = register_slice(0, 3);
            let inputs = &[provider_reg(2), provider_reg(2), provider_reg(2)];
            let actual = TrueCopies::analyze(&mut arena, results, inputs);
            let expected = TrueCopies::Many {
                results: register_slice(0, 2),
                inputs: arena.alloc([provider_reg(2), provider_reg(2)]),
            };
            assert_true_copies_eq(&arena, actual, expected);
        }

        // Case: many true copies at the middle
        //
        // (x0, x1, x2, x3) <- (x0, x3, x3, x3)
        // => (x1, x2) <- (x3, x3)
        {
            let results = register_slice(0, 4);
            let inputs = &[
                provider_reg(0),
                provider_reg(3),
                provider_reg(3),
                provider_reg(3),
            ];
            let actual = TrueCopies::analyze(&mut arena, results, inputs);
            let expected = TrueCopies::Many {
                results: register_slice(1, 2),
                inputs: arena.alloc([provider_reg(3), provider_reg(3)]),
            };
            assert_true_copies_eq(&arena, actual, expected);
        }

        // Case: many true copies at the middle with non-true copies
        //
        // (x0, x1, x2, x3, x4) <- (x0, x4, x2, x4, x4)
        // => (x1, x2, x3) <- (x4, x2, x4)
        {
            let results = register_slice(0, 5);
            let inputs = &[
                provider_reg(0),
                provider_reg(4),
                provider_reg(2),
                provider_reg(4),
                provider_reg(4),
            ];
            let actual = TrueCopies::analyze(&mut arena, results, inputs);
            let expected = TrueCopies::Many {
                results: register_slice(1, 3),
                inputs: arena.alloc([provider_reg(4), provider_reg(2), provider_reg(4)]),
            };
            assert_true_copies_eq(&arena, actual, expected);
        }
    }
}
