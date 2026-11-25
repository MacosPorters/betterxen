# SPDX-License-Identifier: GPL-3.0-or-later
# Original guest instructions. Assembled for inspection only; never a host executable.
.intel_syntax noprefix
.text
addss_probe:
  addss xmm0, xmm1
  ret
paddd_probe:
  paddd xmm0, xmm1
  ret
vpaddd_probe:
  vpaddd ymm0, ymm0, ymm1
  ret
vpxor_probe:
  vpxor xmm0, xmm0, xmm0
  ret
loop_probe:
  inc rax
  dec rcx
  jne loop_probe
  mov [rbx], rax
  ret
bench_register:
  .rept 256
  add rax, rcx
  .endr
  mov [rbx], rax
  ret
bench_memory:
  .rept 256
  add rax, rcx
  mov [rbx], rax
  .endr
  ret
