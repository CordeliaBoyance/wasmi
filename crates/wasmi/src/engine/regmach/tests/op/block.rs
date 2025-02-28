use super::*;
use crate::engine::{
    bytecode::BranchOffset,
    regmach::{
        bytecode::RegisterSpan,
        tests::{display_wasm::DisplayValueType, wasm_type::WasmType},
    },
};
use std::fmt::Display;

#[test]
#[cfg_attr(miri, ignore)]
fn empty_block() {
    let wasm = wat2wasm(
        r"
        (module
            (func (block))
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([Instruction::Return])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn nested_empty_block() {
    let wasm = wat2wasm(
        r"
        (module
            (func (block (block)))
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([Instruction::Return])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn identity_block_1() {
    let wasm = wat2wasm(
        r"
        (module
            (func (param i32) (result i32)
                (local.get 0)
                (block (param i32) (result i32))
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([Instruction::return_reg(Register::from_i16(0))])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn identity_block_2() {
    let wasm = wat2wasm(
        r"
        (module
            (func (param i32 i64) (result i32)
                (local.get 0)
                (local.get 1)
                (block (param i32 i64) (result i32 i64))
                (drop)
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([Instruction::return_reg(Register::from_i16(0))])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn nested_identity_block_1() {
    let wasm = wat2wasm(
        r"
        (module
            (func (param i32) (result i32)
                (local.get 0)
                (block (param i32) (result i32)
                    (block (param i32) (result i32))
                )
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([Instruction::return_reg(Register::from_i16(0))])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn nested_identity_block_2() {
    let wasm = wat2wasm(
        r"
        (module
            (func (param i32 i64) (result i32)
                (local.get 0)
                (local.get 1)
                (block (param i32 i64) (result i32 i64)
                    (block (param i32 i64) (result i32 i64))
                )
                (drop)
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([Instruction::return_reg(Register::from_i16(0))])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn branched_block_0() {
    let wasm = wat2wasm(
        r"
        (module
            (func
                (block
                    (br 0)
                )
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([
            Instruction::branch(BranchOffset::from(1)),
            Instruction::Return,
        ])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn branched_block_1() {
    let wasm = wat2wasm(
        r"
        (module
            (func (param i32) (result i32)
                (local.get 0)
                (block (param i32) (result i32)
                    (br 0)
                )
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([
            Instruction::copy(Register::from_i16(1), Register::from_i16(0)),
            Instruction::branch(BranchOffset::from(1)),
            Instruction::return_reg(Register::from_i16(1)),
        ])
        .run()
}

fn testcase_branched_block_1_imm<T>(value: T) -> TranslationTest
where
    T: Copy + WasmType,
    DisplayWasm<T>: Display,
{
    let display_type = DisplayValueType::from(T::VALUE_TYPE);
    let display_value = DisplayWasm::from(value);
    let wasm = wat2wasm(&format!(
        r"
        (module
            (func (result {display_type})
                (block (result {display_type})
                    ({display_type}.const {display_value})
                    (br 0)
                )
            )
        )",
    ));
    TranslationTest::new(wasm)
}

#[test]
#[cfg_attr(miri, ignore)]
fn branched_block_1_imm_i32() {
    fn test_for_i32(value: i32) {
        testcase_branched_block_1_imm::<i32>(value)
            .expect_func_instrs([
                Instruction::copy_imm32(Register::from_i16(0), AnyConst32::from(value)),
                Instruction::branch(BranchOffset::from(1)),
                Instruction::return_reg(Register::from_i16(0)),
            ])
            .run();
    }
    test_for_i32(0);
    test_for_i32(1);
    test_for_i32(-1);
    test_for_i32(i32::MIN);
    test_for_i32(i32::MAX);
}

#[test]
#[cfg_attr(miri, ignore)]
fn branched_block_1_imm_i64imm32() {
    fn test_for_i64imm32(value: i64) {
        let const32 =
            <Const32<i64>>::from_i64(value).expect("value must be 32-bit encodable for this test");
        testcase_branched_block_1_imm::<i64>(value)
            .expect_func_instrs([
                Instruction::copy_i64imm32(Register::from_i16(0), const32),
                Instruction::branch(BranchOffset::from(1)),
                Instruction::return_reg(Register::from_i16(0)),
            ])
            .run();
    }
    test_for_i64imm32(0);
    test_for_i64imm32(1);
    test_for_i64imm32(-1);
    test_for_i64imm32(i64::from(i32::MIN) + 1);
    test_for_i64imm32(i64::from(i32::MIN));
    test_for_i64imm32(i64::from(i32::MAX) - 1);
    test_for_i64imm32(i64::from(i32::MAX));
}

#[test]
#[cfg_attr(miri, ignore)]
fn branched_block_1_imm_i64() {
    fn test_for_i64(value: i64) {
        testcase_branched_block_1_imm::<i64>(value)
            .expect_func(
                ExpectedFunc::new([
                    Instruction::copy(Register::from_i16(0), Register::from_i16(-1)),
                    Instruction::branch(BranchOffset::from(1)),
                    Instruction::return_reg(Register::from_i16(0)),
                ])
                .consts([value]),
            )
            .run();
    }
    test_for_i64(i64::from(i32::MIN) - 1);
    test_for_i64(i64::from(i32::MAX) + 1);
    test_for_i64(i64::MIN);
    test_for_i64(i64::MAX);
}

#[test]
#[cfg_attr(miri, ignore)]
fn branched_block_1_imm_f32() {
    fn test_for_f32(value: f32) {
        testcase_branched_block_1_imm::<f32>(value)
            .expect_func_instrs([
                Instruction::copy_imm32(Register::from_i16(0), AnyConst32::from(value)),
                Instruction::branch(BranchOffset::from(1)),
                Instruction::return_reg(Register::from_i16(0)),
            ])
            .run();
    }
    test_for_f32(0.0);
    test_for_f32(1.0);
    test_for_f32(-1.0);
    test_for_f32(f32::INFINITY);
    test_for_f32(f32::NEG_INFINITY);
    test_for_f32(f32::NAN);
}

#[test]
#[cfg_attr(miri, ignore)]
fn branched_block_1_imm_f64imm32() {
    fn test_for_f64imm32(value: f64) {
        let const32 = <Const32<f64>>::from_f64(value)
            .expect("value must be losslessly 32-bit encodable for this test");
        testcase_branched_block_1_imm::<f64>(value)
            .expect_func_instrs([
                Instruction::copy_f64imm32(Register::from_i16(0), const32),
                Instruction::branch(BranchOffset::from(1)),
                Instruction::return_reg(Register::from_i16(0)),
            ])
            .run();
    }
    test_for_f64imm32(0.0);
    test_for_f64imm32(-0.25);
    test_for_f64imm32(0.5);
    test_for_f64imm32(1.0);
    test_for_f64imm32(-1.0);
    test_for_f64imm32(f64::INFINITY);
    test_for_f64imm32(f64::NEG_INFINITY);
    test_for_f64imm32(f64::NAN);
    test_for_f64imm32(f64::EPSILON);
}

#[test]
#[cfg_attr(miri, ignore)]
fn branched_block_1_imm_f64() {
    fn test_for_f64(value: f64) {
        testcase_branched_block_1_imm::<f64>(value)
            .expect_func(
                ExpectedFunc::new([
                    Instruction::copy(Register::from_i16(0), Register::from_i16(-1)),
                    Instruction::branch(BranchOffset::from(1)),
                    Instruction::return_reg(Register::from_i16(0)),
                ])
                .consts([value]),
            )
            .run();
    }
    test_for_f64(0.3);
    test_for_f64(0.123456789);
    test_for_f64(0.987654321);
}

#[test]
#[cfg_attr(miri, ignore)]
fn branched_block_2() {
    let wasm = wat2wasm(
        r"
        (module
            (func (param i32 i64) (result i32)
                (local.get 0)
                (local.get 1)
                (block (param i32 i64) (result i32 i64)
                    (br 0)
                )
                (drop)
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([
            Instruction::copy2(RegisterSpan::new(Register::from_i16(2)), 0, 1),
            Instruction::branch(BranchOffset::from(1)),
            Instruction::return_reg(Register::from_i16(2)),
        ])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn branch_if_block_0() {
    let wasm = wat2wasm(
        r"
        (module
            (func (param i32)
                (local.get 0)
                (block (param i32)
                    (br_if 0)
                )
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([
            Instruction::branch_nez(Register::from_i16(0), BranchOffset::from(1)),
            Instruction::Return,
        ])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn branch_if_block_1() {
    let wasm = wat2wasm(
        r"
        (module
            (func (param i32 i32) (result i32)
                (local.get 0)
                (local.get 1)
                (block (param i32 i32) (result i32)
                    (br_if 0)
                )
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([
            Instruction::branch_eqz(Register::from_i16(1), BranchOffset::from(3)),
            Instruction::copy(Register::from_i16(2), Register::from_i16(0)),
            Instruction::branch(BranchOffset::from(2)),
            Instruction::copy(Register::from_i16(2), Register::from_i16(0)),
            Instruction::return_reg(Register::from_i16(2)),
        ])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn branch_to_func_block_0() {
    let wasm = wat2wasm(
        r"
        (module
            (func
                (br 0)
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([Instruction::Return])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn branch_to_func_block_1() {
    let wasm = wat2wasm(
        r"
        (module
            (func (param i32) (result i32)
                (local.get 0)
                (br 0)
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([Instruction::return_reg(Register::from_i16(0))])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn branch_to_func_block_nested_0() {
    let wasm = wat2wasm(
        r"
        (module
            (func
                (block
                    (br 1)
                )
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([Instruction::Return])
        .run()
}

#[test]
#[cfg_attr(miri, ignore)]
fn branch_to_func_block_nested_1() {
    let wasm = wat2wasm(
        r"
        (module
            (func (param i32) (result i32)
                (local.get 0)
                (block (param i32) (result i32)
                    (br 1)
                )
            )
        )",
    );
    TranslationTest::new(wasm)
        .expect_func_instrs([Instruction::return_reg(Register::from_i16(0))])
        .run()
}
