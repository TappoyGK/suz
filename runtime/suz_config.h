#ifndef SUZ_CONFIG_H
#define SUZ_CONFIG_H

#include <float.h>
#include <limits.h>
#include <stdint.h>

#if defined(__FAST_MATH__) || (defined(__FINITE_MATH_ONLY__) && __FINITE_MATH_ONLY__ > 0)
#error "unsafe floating-point optimization [runtime.float-environment]"
#endif

/* Honor the environment initialized at entry and keep each operation separate. */
#pragma STDC FENV_ACCESS ON
#pragma STDC FP_CONTRACT OFF

_Static_assert(CHAR_BIT == 8, "8-bit bytes required [runtime.c-target]");
_Static_assert(FLT_RADIX == 2 && FLT_MANT_DIG == 24 && FLT_MAX_EXP == 128 &&
               FLT_MIN_EXP == -125 && sizeof(float) == sizeof(uint32_t),
               "IEEE binary32 required [runtime.c-target]");
_Static_assert(DBL_MANT_DIG == 53 && DBL_MAX_EXP == 1024 && DBL_MIN_EXP == -1021 &&
               sizeof(double) == sizeof(uint64_t),
               "IEEE binary64 required [runtime.c-target]");
_Static_assert(FLT_EVAL_METHOD == 0,
               "evaluation at the expression type's precision required [runtime.float-environment]");
_Static_assert(FLT_HAS_SUBNORM == 1 && DBL_HAS_SUBNORM == 1,
               "subnormal values required [runtime.c-target]");

#endif
