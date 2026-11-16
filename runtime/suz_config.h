#ifndef SUZ_CONFIG_H
#define SUZ_CONFIG_H

#if !defined(__clang__)
#error "Clang required [runtime.c-check]"
#endif
#if !defined(__STDC_VERSION__) || __STDC_VERSION__ < 202311L
#error "C23 required; compile with -std=c23 [runtime.c-check]"
#endif

#include <float.h>
#include <limits.h>
#include <stdint.h>
#include <stdckdint.h>

#if !defined(BITINT_MAXWIDTH) || BITINT_MAXWIDTH < 128
#error "Clang with 128-bit _BitInt support required [runtime.c-check]"
#endif
#if !defined(__STDC_VERSION_STDCKDINT_H__) || __STDC_VERSION_STDCKDINT_H__ < 202311L
#error "C23 checked integer arithmetic required [runtime.c-check]"
#endif

#if defined(__FAST_MATH__) || (defined(__FINITE_MATH_ONLY__) && __FINITE_MATH_ONLY__ > 0)
#error "unsafe floating-point optimization [runtime.float-environment]"
#endif

/* 入口で初期化した環境に従い、各演算を分離したまま実行する。 */
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
