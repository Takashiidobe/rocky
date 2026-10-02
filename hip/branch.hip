#include <hip/hip_runtime.h>

extern "C" __global__ void branch(unsigned *output) {
  unsigned tid = threadIdx.x;

  if (tid % 2 == 0)
    output[tid] = tid + tid;
  else
    output[tid] = tid * tid;
}
