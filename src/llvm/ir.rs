use inkwell::{
    AddressSpace, IntPredicate, context::Context, types::VectorType, values::BasicValue,
};

use super::instruction::{Instruction, VectorSource};
use crate::error::Error;
use llvm_amdgpu_types::{U32, VectorRegister};

pub fn emit(instructions: Vec<Instruction>) -> Result<String, Error> {
    if instructions.is_empty() {
        return Err(Error::IrConstruction {
            message: "cannot emit an empty instruction list".into(),
        });
    }
    let context = Context::create();
    let module = context.create_module("compare");
    let builder = context.create_builder();
    let i32_type = context.i32_type();
    let wave_type = i32_type.vec_type(32);
    let pointer_type = context.ptr_type(AddressSpace::default());
    let function_type = context.void_type().fn_type(
        &[pointer_type.into(), i32_type.into(), pointer_type.into()],
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
    let exec = parameter(1)?.into_int_value();
    let vcc_lo = parameter(2)?.into_pointer_value();
    vgprs.set_name("vgprs");
    exec.set_name("exec");
    vcc_lo.set_name("vcc_lo");
    builder.position_at_end(context.append_basic_block(function, "entry"));
    let load_register = |register: VectorRegister<U32>, name| {
        let slot = unsafe {
            builder.build_gep(
                wave_type,
                vgprs,
                &[i32_type.const_int(register.index().into(), false)],
                name,
            )?
        };
        let load = builder.build_load(wave_type, slot, name)?;
        load.as_instruction_value()
            .ok_or_else(|| Error::IrConstruction {
                message: "expected a vector register load".into(),
            })?
            .set_alignment(4)?;
        Ok::<_, Error>(load.into_vector_value())
    };
    for instruction in instructions {
        match instruction {
            Instruction::VCmpEqU32 { lhs, rhs, .. } => {
                let lhs = match lhs {
                    VectorSource::Register(register) => load_register(register, "lhs")?,
                    VectorSource::Immediate(value) => {
                        VectorType::const_vector(&[i32_type.const_int(value.into(), false); 32])
                    }
                };
                let rhs = load_register(rhs, "rhs")?;
                let comparison = builder.build_int_compare(IntPredicate::EQ, lhs, rhs, "equal")?;
                let mask = builder
                    .build_bit_cast(comparison, i32_type, "mask")?
                    .into_int_value();
                let active_mask = builder.build_and(mask, exec, "active_mask")?;
                builder.build_store(vcc_lo, active_mask)?;
            }
        }
    }
    builder.build_return(None)?;
    module.verify().map_err(|message| Error::IrVerification {
        message: message.to_string(),
    })?;
    Ok(module.print_to_string().to_string())
}
