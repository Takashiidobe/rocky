# Rocky

## Installing Deps

Install llvm too, i'm on 23.1

```
sudo pacman -Syu hip-runtime-amd rocm-llvm rocm-device-libs rocminfo
```

## Running

Run the compile, extraction, and disassembly pipeline from Rust:

```bash
cargo run -- hip/branch.hip
```

Check out the generated IR:

```bash
cat hip/branch.ll
```

## Testing Flow:

1. Run `hipcc` on a `.hip` file to get a AMD GPU code object (.hsaco)
2. use llvm-objdump to get back the asm
3. build CFG and lift to LLVM IR
4. Compile back to .hsaco to get back the gpu code
5. fuzz both to compare em

## Instruction types

[llvm-amdgpu-types](../llvm-amdgpu-types/README.md) generates generic instruction
families and a shared decoder from LLVM TableGen. Typed operands distinguish
numeric interpretation, register class, encoding, and wave size.

The AMDGPU front end lowers its supported instruction forms into wave32 LLVM
IR. Other decoded forms are logged and skipped. `--strict` reports the first
unsupported instruction with its objdump source span:

```bash
cargo run -- --strict hip/branch.hip
```

The AMDGPU parser turns disassembly into an instruction representation, and the
LLVM lowering converts that representation into LLVM IR. Skipping instructions
produces partial IR rather than a translation of the whole kernel.
