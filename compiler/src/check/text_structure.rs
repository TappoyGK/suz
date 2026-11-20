//! 構造体のスキーマをCへ渡し、文字列から構造体への定数変換をランタイムと同じパーサーで評価する。
//!
//! 利用元: [`crate::check::constants`]。

use super::{
    ir::{CheckedExpression, Value},
    scope::Scope,
};
use crate::ast::{FloatType, Type};
use std::{
    ffi::{CString, c_char},
    ptr,
};

#[repr(C)]
struct RawType {
    kind: u32,
    bits: u32,
    signed_integer: bool,
    name: *const c_char,
    fields: *const RawField,
    count: usize,
    slots: usize,
}
#[repr(C)]
struct RawField {
    name: *const c_char,
    ty: *const RawType,
    slot: usize,
}
#[repr(C)]
#[derive(Default, Clone)]
struct RawValue {
    bits: u64,
    start: u64,
    length: u64,
    seen: bool,
}

#[derive(Default)]
struct Schema {
    // ベクターが伸びても、BoxとCStringの指す領域のアドレスは変わらない。
    types: Vec<(Type, Box<RawType>)>,
    fields: Vec<Box<[RawField]>>,
    names: Vec<CString>,
}
impl Schema {
    fn name(&mut self, name: &str) -> *const c_char {
        let name = CString::new(name).unwrap();
        let pointer = name.as_ptr();
        self.names.push(name);
        pointer
    }
    fn build(&mut self, ty: Type, scope: &Scope) -> (*const RawType, usize) {
        if let Some((_, entry)) = self.types.iter().find(|(existing, _)| *existing == ty) {
            return (entry.as_ref(), entry.slots);
        }
        let mut entry = RawType {
            kind: 0,
            bits: 0,
            signed_integer: false,
            name: ptr::null(),
            fields: ptr::null(),
            count: 0,
            slots: 1,
        };
        match ty {
            Type::Integer(ty) => {
                entry.bits = ty.bits();
                entry.signed_integer = ty.bounds().0 < 0;
            }
            Type::Float(ty) => {
                entry.kind = 1;
                entry.bits = if ty == FloatType::F32 { 32 } else { 64 };
            }
            Type::Char => entry.kind = 6,
            Type::Bool => entry.kind = 2,
            Type::Symbol | Type::Text => entry.kind = 3,
            Type::Enum(ty) => {
                entry.kind = 4;
                let declaration = &scope.enums[ty.id];
                entry.name = self.name(&declaration.name);
                let fields: Box<[_]> = declaration
                    .variants
                    .iter()
                    .map(|name| RawField {
                        name: self.name(name),
                        ty: ptr::null(),
                        slot: 0,
                    })
                    .collect();
                entry.fields = fields.as_ptr();
                entry.count = fields.len();
                self.fields.push(fields);
            }
            Type::Struct(id) => {
                entry.kind = 5;
                entry.slots = 0;
                let declaration = &scope.structs[id];
                entry.name = self.name(&declaration.name);
                let mut fields = Vec::new();
                for field in &declaration.fields {
                    let (child, slots) = self.build(field.ty, scope);
                    fields.push(RawField {
                        name: self.name(&field.name),
                        ty: child,
                        slot: entry.slots,
                    });
                    entry.slots += slots;
                }
                let fields = fields.into_boxed_slice();
                entry.fields = fields.as_ptr();
                entry.count = fields.len();
                self.fields.push(fields);
            }
            Type::Pointer { .. }
            | Type::CPointer
            | Type::CInteger(_)
            | Type::StaticFunction(_)
            | Type::Function(_)
            | Type::Parameter(_)
            | Type::GenericInt
            | Type::GenericFloat
            | Type::Numeric(_)
            | Type::Exact
            | Type::Void => {
                unreachable!("text conversions only receive resolved, storable types")
            }
        }
        let entry = Box::new(entry);
        let pointer = entry.as_ref() as *const RawType;
        let slots = entry.slots;
        self.types.push((ty, entry));
        (pointer, slots)
    }
}

unsafe extern "C" {
    fn suz_parse_struct(
        bytes: *const u8,
        length: u64,
        schema: *const RawType,
        output: *mut RawValue,
    ) -> bool;
    fn suz_parse_string(
        bytes: *const u8,
        length: u64,
        consumed: *mut u64,
        output: *mut u8,
        decoded: *mut u64,
    ) -> bool;
}

pub(super) fn parse(text: &str, id: usize, scope: &Scope) -> Option<CheckedExpression> {
    let mut schema = Schema::default();
    let (root, slots) = schema.build(Type::Struct(id), scope);
    let mut values = vec![RawValue::default(); slots];
    // 呼び出し中は、すべてのスキーマの参照先と出力配列全体が生存する。
    if !unsafe { suz_parse_struct(text.as_ptr(), text.len() as u64, root, values.as_mut_ptr()) } {
        return None;
    }
    fn convert(
        ty: Type,
        values: &mut std::slice::Iter<'_, RawValue>,
        text: &str,
        scope: &Scope,
    ) -> Option<CheckedExpression> {
        if let Type::Struct(id) = ty {
            let fields = scope.structs[id]
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| Some((index, convert(field.ty, values, text, scope)?)))
                .collect::<Option<_>>()?;
            return Some(CheckedExpression {
                ty,
                value: Value::Struct { fields },
            });
        }
        let raw = values.next()?;
        let value = match ty {
            Type::Integer(ty) => super::text_parse::integer_value(raw.bits, ty),
            Type::Enum(_) | Type::Char => Value::Integer {
                value: i128::from(raw.bits),
                minus: None,
            },
            Type::Float(_) => Value::Float(raw.bits),
            Type::Bool => Value::Bool(raw.bits != 0),
            Type::Symbol | Type::Text => {
                let input = text
                    .as_bytes()
                    .get(raw.start as usize..(raw.start + raw.length) as usize)?;
                let mut decoded = vec![0_u8; input.len()];
                let (mut consumed, mut length) = (0, 0);
                // この引用付き範囲は検証済みで、出力先にはinput.len()バイトある。
                let valid = unsafe {
                    suz_parse_string(
                        input.as_ptr(),
                        input.len() as u64,
                        &mut consumed,
                        decoded.as_mut_ptr(),
                        &mut length,
                    )
                };
                if !valid || consumed != raw.length {
                    return None;
                }
                decoded.truncate(length as usize);
                Value::Text {
                    text: String::from_utf8(decoded).ok()?,
                }
            }
            _ => unreachable!(),
        };
        if ty == Type::Text {
            return Some(CheckedExpression {
                ty,
                value: Value::TextView(Box::new(CheckedExpression {
                    ty: Type::Symbol,
                    value,
                })),
            });
        }
        Some(CheckedExpression { ty, value })
    }
    convert(Type::Struct(id), &mut values.iter(), text, scope)
}
