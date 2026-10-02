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
