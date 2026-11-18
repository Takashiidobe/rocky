use inkwell::{
    AddressSpace, IntPredicate, context::Context, types::VectorType, values::BasicValue,
};
use llvm_amdgpu_types::{
    B32, B64, OperandType, ScalarRegister, ScalarRegisterOperand, ScalarSourceOperand,
    SpecialRegisterName, VectorRegister,
};
use std::num::NonZeroU32;

use crate::{
    error::Error,
    llvm::instruction::{Instruction, ScalarSource, VectorSource},
};

struct RegisterState<'ctx> {
    vgprs: inkwell::values::PointerValue<'ctx>,
    sgprs: inkwell::values::PointerValue<'ctx>,
    exec_lo: inkwell::values::PointerValue<'ctx>,
    exec_hi: inkwell::values::PointerValue<'ctx>,
    vcc_lo: inkwell::values::PointerValue<'ctx>,
    vcc_hi: inkwell::values::PointerValue<'ctx>,
    scc: inkwell::values::PointerValue<'ctx>,
    m0: inkwell::values::PointerValue<'ctx>,
}

struct IrEmitter<'ctx, 'builder> {
    context: &'ctx Context,
    module: &'builder inkwell::module::Module<'ctx>,
    builder: &'builder inkwell::builder::Builder<'ctx>,
    registers: RegisterState<'ctx>,
}

pub fn emit(instructions: Vec<Instruction>) -> Result<String, Error> {
    if instructions.is_empty() {
        return Err(Error::IrConstruction {
            message: "cannot emit an empty instruction list".into(),
        });
    }
    let context = Context::create();
    let module = context.create_module("lifted");
    let builder = context.create_builder();
    let pointer_type = context.ptr_type(AddressSpace::default());
    let function_type = context.void_type().fn_type(
        &[
            pointer_type.into(),
            pointer_type.into(),
            pointer_type.into(),
            pointer_type.into(),
            pointer_type.into(),
            pointer_type.into(),
            pointer_type.into(),
            pointer_type.into(),
        ],
        false,
    );
    let function = module.add_function("lifted", function_type, None);
    let parameter = |index| {
        function
            .get_nth_param(index)
            .ok_or_else(|| Error::IrConstruction {
                message: format!("missing parameter {index}"),
            })
    };
    let vgprs = parameter(0)?.into_pointer_value();
    let sgprs = parameter(1)?.into_pointer_value();
    let exec_lo = parameter(2)?.into_pointer_value();
    let exec_hi = parameter(3)?.into_pointer_value();
    let vcc_lo = parameter(4)?.into_pointer_value();
    let vcc_hi = parameter(5)?.into_pointer_value();
    let scc = parameter(6)?.into_pointer_value();
    let m0 = parameter(7)?.into_pointer_value();
    for (value, name) in [
        (vgprs, "vgprs"),
        (sgprs, "sgprs"),
        (exec_lo, "exec_lo"),
        (exec_hi, "exec_hi"),
        (vcc_lo, "vcc_lo"),
        (vcc_hi, "vcc_hi"),
        (scc, "scc"),
        (m0, "m0"),
    ] {
        value.set_name(name);
    }
    let emitter = IrEmitter {
        context: &context,
        module: &module,
        builder: &builder,
        registers: RegisterState {
            vgprs,
            sgprs,
            exec_lo,
            exec_hi,
            vcc_lo,
            vcc_hi,
            scc,
            m0,
        },
    };
    builder.position_at_end(context.append_basic_block(function, "entry"));
    emitter.emit_instructions(instructions)?;
    module.verify().map_err(|message| Error::IrVerification {
        message: message.to_string(),
    })?;
    Ok(module.print_to_string().to_string())
}

impl<'ctx, 'builder> IrEmitter<'ctx, 'builder> {
    fn emit_instructions(&self, instructions: Vec<Instruction>) -> Result<(), Error> {
        let context = self.context;
        let module = self.module;
        let builder = self.builder;
        let i32_type = context.i32_type();
        let exec_lo = self.registers.exec_lo;
        let vcc_lo = self.registers.vcc_lo;
        let scc = self.registers.scc;
        let mut terminated = false;
        for instruction in instructions {
            match instruction {
                Instruction::VAndB32E32 { dest, lhs, rhs } => {
                    let lhs = self.load_vector_source(lhs, "and_lhs")?;
                    let rhs = self.load_vector(rhs, "and_rhs")?;
                    let result = builder.build_and(lhs, rhs, "and")?;
                    self.store_vector_active(dest, result, "and_dest")?;
                }
                Instruction::VCmpEqU32 { lhs, rhs } => {
                    let lhs = self.load_vector_source(lhs, "cmp_lhs")?;
                    let rhs = self.load_vector(rhs, "cmp_rhs")?;
                    let comparison =
                        builder.build_int_compare(IntPredicate::EQ, lhs, rhs, "equal")?;
                    let mask = builder
                        .build_bit_cast(comparison, i32_type, "mask")?
                        .into_int_value();
                    let exec = self.load_special(exec_lo, "exec")?;
                    let active_mask = builder.build_and(mask, exec, "active_mask")?;
                    self.store_special(vcc_lo, active_mask, "vcc_lo")?;
                }
                Instruction::SAndSaveexecB32 { src, dest } => {
                    let old_exec = self.load_special(exec_lo, "old_exec")?;
                    let src = self.load_scalar_source(src, "and_saveexec_src")?;
                    self.store_scalar(dest, old_exec, "and_saveexec_dest")?;
                    let next_exec = builder.build_and(old_exec, src, "next_exec")?;
                    self.store_special(exec_lo, next_exec, "exec_lo")?;
                    let scc_value = builder.build_int_compare(
                        IntPredicate::NE,
                        next_exec,
                        i32_type.const_zero(),
                        "exec_scc",
                    )?;
                    let scc_value =
                        builder.build_int_z_extend(scc_value, i32_type, "exec_scc_i32")?;
                    self.store_special(scc, scc_value, "scc")?;
                }
                Instruction::SXorB32 { dest, lhs, rhs } => {
                    let lhs = self.load_scalar_source(lhs, "xor_lhs")?;
                    let rhs = self.load_scalar_source(rhs, "xor_rhs")?;
                    let result = builder.build_xor(lhs, rhs, "xor")?;
                    let scc_value = builder.build_int_compare(
                        IntPredicate::NE,
                        result,
                        i32_type.const_zero(),
                        "xor_scc",
                    )?;
                    let scc_value =
                        builder.build_int_z_extend(scc_value, i32_type, "xor_scc_i32")?;
                    self.store_scalar_destination(dest, result, "xor_dest")?;
                    self.store_special(scc, scc_value, "scc")?;
                }
                Instruction::SOrB32 { dest, lhs, rhs } => {
                    let lhs = self.load_scalar_source(lhs, "or_lhs")?;
                    let rhs = self.load_scalar_source(rhs, "or_rhs")?;
                    let result = builder.build_or(lhs, rhs, "or")?;
                    let scc_value = builder.build_int_compare(
                        IntPredicate::NE,
                        result,
                        i32_type.const_zero(),
                        "or_scc",
                    )?;
                    let scc_value =
                        builder.build_int_z_extend(scc_value, i32_type, "or_scc_i32")?;
                    self.store_scalar_destination(dest, result, "or_dest")?;
                    self.store_special(scc, scc_value, "scc")?;
                }
                Instruction::VMulU32U24E32 { dest, lhs, rhs } => {
                    let lhs = self.load_vector_source(lhs, "mul_lhs")?;
                    let rhs = self.load_vector(rhs, "mul_rhs")?;
                    let mask =
                        VectorType::const_vector(&[i32_type.const_int(0x00ff_ffff, false); 32]);
                    let lhs = builder.build_and(lhs, mask, "mul_lhs_u24")?;
                    let rhs = builder.build_and(rhs, mask, "mul_rhs_u24")?;
                    let result = builder.build_int_mul(lhs, rhs, "mul")?;
                    self.store_vector_active(dest, result, "mul_dest")?;
                }
                Instruction::SAndn2saveexecB32 { src, dest } => {
                    let old_exec = self.load_special(exec_lo, "old_exec")?;
                    let src = self.load_scalar_source(src, "andn2_saveexec_src")?;
                    self.store_scalar(dest, old_exec, "andn2_saveexec_dest")?;
                    let inverted = builder.build_not(old_exec, "inverted_exec")?;
                    let next_exec = builder.build_and(src, inverted, "next_exec")?;
                    self.store_special(exec_lo, next_exec, "exec_lo")?;
                    let scc_value = builder.build_int_compare(
                        IntPredicate::NE,
                        next_exec,
                        i32_type.const_zero(),
                        "exec_scc",
                    )?;
                    let scc_value =
                        builder.build_int_z_extend(scc_value, i32_type, "exec_scc_i32")?;
                    self.store_special(scc, scc_value, "scc")?;
                }
                Instruction::VLshlrevB32E32 { dest, lhs, rhs } => {
                    let amount = self.load_vector_source(lhs, "shift_amount")?;
                    let value = self.load_vector(rhs, "shift_value")?;
                    let amount = builder.build_and(
                        amount,
                        VectorType::const_vector(&[i32_type.const_int(31, false); 32]),
                        "shift_amount_low5",
                    )?;
                    let result = builder.build_left_shift(value, amount, "shift")?;
                    self.store_vector_active(dest, result, "shift_dest")?;
                }
                Instruction::SLoadDwordx2 { dest, base, offset } => {
                    let base = self.load_scalar64(base, "load_base")?;
                    let address = if let Some(register) = offset.register() {
                        let register = self.load_scalar(register, "load_offset_reg")?;
                        let register = builder.build_int_z_extend(
                            register,
                            context.i64_type(),
                            "load_offset_ext",
                        )?;
                        builder.build_int_add(base, register, "load_address_reg")?
                    } else {
                        base
                    };
                    let address = builder.build_int_add(
                        address,
                        context.i64_type().const_int(offset.offset() as u64, false),
                        "load_address",
                    )?;
                    let pointer = builder.build_int_to_ptr(
                        address,
                        context.ptr_type(AddressSpace::default()),
                        "load_ptr",
                    )?;
                    let value = builder
                        .build_load(context.i64_type(), pointer, "loaded_pair")?
                        .into_int_value();
                    self.store_scalar64(dest, value, "load_dest")?;
                }
                Instruction::SWaitcnt => {}
                Instruction::GlobalStoreDword { vaddr, data, saddr } => {
                    let base = self.load_scalar64(saddr, "store_base")?;
                    let addresses = self.load_vector(vaddr, "store_addresses")?;
                    let values = self.load_vector(data, "store_values")?;
                    let base_vector = addresses.get_type().const_zero();
                    let base_vector = builder.build_insert_element(
                        base_vector,
                        base,
                        i32_type.const_zero(),
                        "store_base_lane",
                    )?;
                    let splat_mask = VectorType::const_vector(&[i32_type.const_zero(); 32]);
                    let base_vector = builder.build_shuffle_vector(
                        base_vector,
                        base_vector,
                        splat_mask,
                        "store_base_splat",
                    )?;
                    let addresses =
                        builder.build_int_add(base_vector, addresses, "store_addresses")?;
                    let pointer_vector_type =
                        context.ptr_type(AddressSpace::default()).vec_type(32);
                    let pointers = builder.build_int_to_ptr(
                        addresses,
                        pointer_vector_type,
                        "store_pointers",
                    )?;
                    let lane_mask = builder.build_bit_cast(
                        self.load_special(exec_lo, "exec")?,
                        context.bool_type().vec_type(32),
                        "store_mask",
                    )?;
                    let scatter_name = "llvm.masked.scatter.v32i32.v32p0";
                    let scatter = module.get_function(scatter_name).unwrap_or_else(|| {
                        let scatter_type = context.void_type().fn_type(
                            &[
                                values.get_type().into(),
                                pointer_vector_type.into(),
                                lane_mask.get_type().into(),
                            ],
                            false,
                        );
                        module.add_function(scatter_name, scatter_type, None)
                    });
                    builder.build_call(
                        scatter,
                        &[values.into(), pointers.into(), lane_mask.into()],
                        "",
                    )?;
                }
                Instruction::SEndpgm | Instruction::SCodeEnd => {
                    builder.build_return(None)?;
                    terminated = true;
                    break;
                }
            }
        }
        if !terminated {
            builder.build_return(None)?;
        }
        Ok(())
    }

    fn load_vector<T: OperandType>(
        &self,
        register: VectorRegister<T>,
        name: &str,
    ) -> Result<inkwell::values::VectorValue<'ctx>, Error> {
        let context = self.context;
        let builder = self.builder;
        if T::BITS == 64 {
            let low = self.load_vector_raw(32, register.index(), "vgpr_low")?;
            let high = self.load_vector_raw(32, register.index() + 1, "vgpr_high")?;
            let mask: Vec<_> = (0..64)
                .map(|index| {
                    context.i32_type().const_int(
                        if index % 2 == 0 {
                            index / 2
                        } else {
                            32 + index / 2
                        },
                        false,
                    )
                })
                .collect();
            let mask = VectorType::const_vector(&mask);
            let interleaved = builder.build_shuffle_vector(low, high, mask, "vgpr_pair_words")?;
            return Ok(builder
                .build_bit_cast(interleaved, context.i64_type().vec_type(32), name)?
                .into_vector_value());
        }
        self.load_vector_raw(u32::from(T::BITS), register.index(), name)
    }

    fn load_vector_raw(
        &self,
        bits: u32,
        index: u16,
        name: &str,
    ) -> Result<inkwell::values::VectorValue<'ctx>, Error> {
        let context = self.context;
        let builder = self.builder;
        let vgprs = self.registers.vgprs;
        let element_type = context
            .custom_width_int_type(NonZeroU32::new(bits).unwrap())
            .expect("operand width is supported by LLVM");
        let vector_type = element_type.vec_type(32);
        let slot = unsafe {
            builder.build_gep(
                vector_type,
                vgprs,
                &[context.i32_type().const_int(index.into(), false)],
                name,
            )?
        };
        let value = builder.build_load(vector_type, slot, name)?;
        value
            .as_instruction_value()
            .ok_or_else(|| Error::IrConstruction {
                message: "expected a vector register load".into(),
            })?
            .set_alignment(4)?;
        Ok(value.into_vector_value())
    }

    fn load_vector_source<T: OperandType<Value = u32>>(
        &self,
        source: VectorSource<T>,
        name: &str,
    ) -> Result<inkwell::values::VectorValue<'ctx>, Error> {
        let context = self.context;
        let element_type = context
            .custom_width_int_type(NonZeroU32::new(u32::from(T::BITS)).unwrap())
            .expect("operand width is supported by LLVM");
        Ok(match source {
            VectorSource::Register(register) => self.load_vector(register, name)?,
            VectorSource::Immediate(value) => {
                VectorType::const_vector(&[element_type.const_int(value.into(), false); 32])
            }
        })
    }

    fn store_vector<T: OperandType>(
        &self,
        register: VectorRegister<T>,
        value: inkwell::values::VectorValue<'ctx>,
        name: &str,
    ) -> Result<(), Error> {
        let context = self.context;
        let builder = self.builder;
        if T::BITS == 64 {
            let words = builder
                .build_bit_cast(value, context.i32_type().vec_type(64), "vgpr_pair_words")?
                .into_vector_value();
            let low_mask = VectorType::const_vector(
                &(0..32)
                    .map(|index| context.i32_type().const_int(index * 2, false))
                    .collect::<Vec<_>>(),
            );
            let high_mask = VectorType::const_vector(
                &(0..32)
                    .map(|index| context.i32_type().const_int(index * 2 + 1, false))
                    .collect::<Vec<_>>(),
            );
            let low = builder.build_shuffle_vector(words, words, low_mask, "vgpr_low_vec")?;
            let high = builder.build_shuffle_vector(words, words, high_mask, "vgpr_high_vec")?;
            self.store_vector_raw(register.index(), low, "vgpr_low_store")?;
            return self.store_vector_raw(register.index() + 1, high, name);
        }
        self.store_vector_raw(register.index(), value, name)
    }

    fn store_vector_raw(
        &self,
        index: u16,
        value: inkwell::values::VectorValue<'ctx>,
        name: &str,
    ) -> Result<(), Error> {
        let context = self.context;
        let builder = self.builder;
        let vgprs = self.registers.vgprs;
        let vector_type = value.get_type();
        let slot = unsafe {
            builder.build_gep(
                vector_type,
                vgprs,
                &[context.i32_type().const_int(index.into(), false)],
                name,
            )?
        };
        builder.build_store(slot, value)?;
        Ok(())
    }

    fn store_vector_active<T: OperandType>(
        &self,
        register: VectorRegister<T>,
        value: inkwell::values::VectorValue<'ctx>,
        name: &str,
    ) -> Result<(), Error> {
        let old = self.load_vector(register, "old_dest")?;
        let exec = self.load_special(self.registers.exec_lo, "exec")?;
        let context = self.context;
        let builder = self.builder;
        let mask = builder
            .build_bit_cast(exec, context.bool_type().vec_type(32), "lane_mask")?
            .into_vector_value();
        let value = builder
            .build_select(mask, value, old, name)?
            .into_vector_value();
        self.store_vector(register, value, name)
    }

    fn load_scalar<T: OperandType<Value = u32>>(
        &self,
        register: ScalarRegister<T>,
        name: &str,
    ) -> Result<inkwell::values::IntValue<'ctx>, Error> {
        let context = self.context;
        let builder = self.builder;
        let sgprs = self.registers.sgprs;
        let slot = unsafe {
            builder.build_gep(
                context.i32_type(),
                sgprs,
                &[context.i32_type().const_int(register.index().into(), false)],
                name,
            )?
        };
        Ok(builder
            .build_load(context.i32_type(), slot, name)?
            .into_int_value())
    }

    fn store_scalar(
        &self,
        register: ScalarRegister<B32>,
        value: inkwell::values::IntValue<'ctx>,
        name: &str,
    ) -> Result<(), Error> {
        let context = self.context;
        let builder = self.builder;
        let sgprs = self.registers.sgprs;
        let slot = unsafe {
            builder.build_gep(
                context.i32_type(),
                sgprs,
                &[context.i32_type().const_int(register.index().into(), false)],
                name,
            )?
        };
        builder.build_store(slot, value)?;
        Ok(())
    }

    fn load_scalar64(
        &self,
        register: ScalarRegister<B64>,
        name: &str,
    ) -> Result<inkwell::values::IntValue<'ctx>, Error> {
        let context = self.context;
        let builder = self.builder;
        let sgprs = self.registers.sgprs;
        let slot = unsafe {
            builder.build_gep(
                context.i64_type(),
                sgprs,
                &[context
                    .i32_type()
                    .const_int((register.index() / 2).into(), false)],
                name,
            )?
        };
        Ok(builder
            .build_load(context.i64_type(), slot, name)?
            .into_int_value())
    }

    fn store_scalar64(
        &self,
        register: ScalarRegister<B64>,
        value: inkwell::values::IntValue<'ctx>,
        name: &str,
    ) -> Result<(), Error> {
        let context = self.context;
        let builder = self.builder;
        let sgprs = self.registers.sgprs;
        let slot = unsafe {
            builder.build_gep(
                context.i64_type(),
                sgprs,
                &[context
                    .i32_type()
                    .const_int((register.index() / 2).into(), false)],
                name,
            )?
        };
        builder.build_store(slot, value)?;
        Ok(())
    }

    fn load_scalar_source(
        &self,
        source: ScalarSource<B32>,
        name: &str,
    ) -> Result<inkwell::values::IntValue<'ctx>, Error> {
        let context = self.context;
        match source {
            ScalarSource::Register(register) => self.load_scalar(register, name),
            ScalarSource::Immediate(value) => Ok(context.i32_type().const_int(value.into(), false)),
            ScalarSource::SpecialRegister(register) => {
                let pointer = match register.name() {
                    SpecialRegisterName::VccLo => self.registers.vcc_lo,
                    SpecialRegisterName::VccHi => self.registers.vcc_hi,
                    SpecialRegisterName::ExecLo => self.registers.exec_lo,
                    SpecialRegisterName::ExecHi => self.registers.exec_hi,
                    SpecialRegisterName::Scc => self.registers.scc,
                    SpecialRegisterName::M0 => self.registers.m0,
                    SpecialRegisterName::Vcc | SpecialRegisterName::Exec => {
                        return Err(Error::IrConstruction {
                            message: "64-bit special register used as a 32-bit source".into(),
                        });
                    }
                };
                self.load_special(pointer, name)
            }
        }
    }

    fn load_special(
        &self,
        pointer: inkwell::values::PointerValue<'ctx>,
        name: &str,
    ) -> Result<inkwell::values::IntValue<'ctx>, Error> {
        Ok(self
            .builder
            .build_load(self.context.i32_type(), pointer, name)?
            .into_int_value())
    }

    fn store_special(
        &self,
        pointer: inkwell::values::PointerValue<'ctx>,
        value: inkwell::values::IntValue<'ctx>,
        _name: &str,
    ) -> Result<(), Error> {
        self.builder.build_store(pointer, value)?;
        Ok(())
    }

    fn store_scalar_destination(
        &self,
        destination: ScalarRegisterOperand<B32>,
        value: inkwell::values::IntValue<'ctx>,
        name: &str,
    ) -> Result<(), Error> {
        match destination.operand() {
            ScalarSourceOperand::Register(register) => self.store_scalar(register, value, name),
            ScalarSourceOperand::SpecialRegister(register) => {
                let pointer = match register.name() {
                    SpecialRegisterName::VccLo => self.registers.vcc_lo,
                    SpecialRegisterName::VccHi => self.registers.vcc_hi,
                    SpecialRegisterName::ExecLo => self.registers.exec_lo,
                    SpecialRegisterName::ExecHi => self.registers.exec_hi,
                    SpecialRegisterName::M0 => self.registers.m0,
                    SpecialRegisterName::Scc => self.registers.scc,
                    SpecialRegisterName::Vcc | SpecialRegisterName::Exec => {
                        return Err(Error::IrConstruction {
                            message: "64-bit special register used as a 32-bit destination".into(),
                        });
                    }
                };
                self.store_special(pointer, value, name)
            }
            ScalarSourceOperand::Immediate(_) => Err(Error::IrConstruction {
                message: "immediate scalar destination".into(),
            }),
        }
    }
}
