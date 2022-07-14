use super::{bytecode::ExecRegister, ConstRef};
use crate::arena::Index;
use alloc::collections::{btree_map::Entry, BTreeMap};
use core::ops::Neg;
use wasmi_core::UntypedValue;

/// A deduplicating [`ExecProviderSlice`] arena.
#[derive(Debug, Default)]
pub struct DedupProviderSliceArena {
    dedup: BTreeMap<Box<[ExecProvider]>, ExecProviderSlice>,
    providers: Vec<ExecProvider>,
}

impl DedupProviderSliceArena {
    /// Allocates a new [`ExecProviderSlice`] consisting of the given registers.
    pub fn alloc<T>(&mut self, providers: T) -> ExecProviderSlice
    where
        T: IntoIterator<Item = ExecProvider>,
    {
        let providers: Box<[ExecProvider]> = providers.into_iter().collect();
        match self.dedup.entry(providers) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let new_providers: &[ExecProvider] = entry.key();
                let first: u16 = self.providers.len().try_into().unwrap_or_else(|error| {
                    panic!(
                        "out of bounds index of {} for provider slice: {error}",
                        self.providers.len()
                    )
                });
                let len: u16 = new_providers.len().try_into().unwrap_or_else(|error| {
                    panic!(
                        "register slice of length {} too long: {error}",
                        new_providers.len()
                    )
                });
                self.providers.extend_from_slice(new_providers);
                let dedup = ExecProviderSlice { first, len };
                entry.insert(dedup);
                dedup
            }
        }
    }

    /// Resolves a [`ExecProviderSlice`] to its underlying registers or immediates.
    pub fn resolve(&self, slice: ExecProviderSlice) -> &[ExecProvider] {
        let first = slice.first as usize;
        let len = slice.len as usize;
        &self.providers[first..first + len]
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ExecProviderSlice {
    first: u16,
    len: u16,
}

impl ExecProviderSlice {
    /// Creates a new [`ExecProviderSlice`] with the given properties.
    #[cfg(test)]
    pub fn new(first: u16, len: u16) -> Self {
        Self { first, len }
    }

    /// Creates a new empty [`ExecProviderSlice`].
    #[cfg(test)]
    pub fn empty() -> Self {
        Self::new(0, 0)
    }

    /// Returns the number of [`ExecProvider`]s in the [`ExecProviderSlice`].
    pub fn len(&self) -> usize {
        self.len as usize
    }
}

/// Either an [`ExecRegister`] or a [`ConstRef`] input value.
///
/// # Developer Note
///
/// Negative numbers represent an index into the constant table
/// and positive numbers represent the index of a register.
/// Both, indices into the constant table and indices of registers
/// are `u16`, therefore it is possible to represent them using a
/// value of type `i32`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExecProvider(i32);

impl From<ExecRegister> for ExecProvider {
    fn from(register: ExecRegister) -> Self {
        Self::from_register(register)
    }
}

impl From<ConstRef> for ExecProvider {
    fn from(const_ref: ConstRef) -> Self {
        Self::from_immediate(const_ref)
    }
}

impl ExecProvider {
    pub fn from_register(register: ExecRegister) -> Self {
        let inner = register.into_inner() as u32 as i32;
        Self(inner)
    }

    pub fn from_immediate(immediate: ConstRef) -> Self {
        let index = immediate.into_inner();
        assert!(
            index < i32::MAX as u32,
            "encountered out of bounds constant index: {index}"
        );
        let inner = (index as i32).wrapping_add(1).neg();
        Self(inner)
    }
}

impl ExecProvider {
    pub fn decode(self) -> RegisterOrImmediate {
        if self.0.is_negative() {
            return ConstRef::from_usize(self.0.abs().wrapping_sub(1) as usize).into();
        }
        ExecRegister::from_inner(self.0 as u16).into()
    }

    pub fn decode_using(
        self,
        resolve_register: impl FnOnce(ExecRegister) -> UntypedValue,
        resolve_const: impl FnOnce(ConstRef) -> UntypedValue,
    ) -> UntypedValue {
        match self.decode() {
            RegisterOrImmediate::Register(register) => resolve_register(register),
            RegisterOrImmediate::Immediate(cref) => resolve_const(cref),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RegisterOrImmediate {
    Register(ExecRegister),
    Immediate(ConstRef),
}

impl From<ExecRegister> for RegisterOrImmediate {
    fn from(register: ExecRegister) -> Self {
        Self::Register(register)
    }
}

impl From<ConstRef> for RegisterOrImmediate {
    fn from(immediate: ConstRef) -> Self {
        Self::Immediate(immediate)
    }
}
