//! 配列の要素型・次元数・添字列挙型を共通化する。実行時の長さの値は型に含めない。
use super::{EnumType, Type};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ArrayType {
    pub element: Type,
    pub rank: usize,
    pub index_enum: Option<EnumType>,
    pub soa: bool,
}

#[derive(Clone, Default)]
pub(crate) struct Types {
    pub entries: Vec<ArrayType>,
    ids: BTreeMap<ArrayType, usize>,
}
impl Types {
    pub fn intern_full(
        &mut self,
        element: Type,
        rank: usize,
        index_enum: Option<EnumType>,
        soa: bool,
    ) -> Type {
        let array = ArrayType {
            element,
            rank,
            index_enum,
            soa,
        };
        let id = *self.ids.entry(array).or_insert_with(|| {
            let id = self.entries.len();
            self.entries.push(array);
            id
        });
        Type::Array(id)
    }
    pub fn truncate(&mut self, length: usize) {
        self.entries.truncate(length);
        self.ids.retain(|_, id| *id < length);
    }
    pub fn get(&self, id: usize) -> ArrayType {
        self.entries[id]
    }
    pub fn patch_ranges(&mut self, ranges: &[super::RangeDeclaration]) {
        self.ids.clear();
        for (id, entry) in self.entries.iter_mut().enumerate() {
            entry.element.patch_ranges(ranges);
            self.ids.insert(*entry, id);
        }
    }
}

/// 型の同一性には含めず、仮引数または不変の参照の宣言で束縛する寸法名。
#[derive(Clone)]
pub(crate) struct Dimension {
    pub name: String,
    pub index: usize,
    pub location: super::Location,
}

/// SoA 配列の葉列（元の構造体から葉までのフィールド番号経路と葉の型）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SoaLeaf {
    pub path: Vec<usize>,
    pub ty: Type,
}

/// 構造体 `struct_id` の全葉列を宣言順に展開する。
pub(crate) fn soa_leaves(
    struct_id: usize,
    get_fields: &impl Fn(usize) -> Vec<Type>,
) -> Vec<SoaLeaf> {
    fn walk(
        struct_id: usize,
        prefix: &mut Vec<usize>,
        get_fields: &impl Fn(usize) -> Vec<Type>,
        out: &mut Vec<SoaLeaf>,
    ) {
        for (i, ty) in get_fields(struct_id).into_iter().enumerate() {
            prefix.push(i);
            if let Type::Struct(sub_id) = ty {
                walk(sub_id, prefix, get_fields, out);
            } else {
                out.push(SoaLeaf {
                    path: prefix.clone(),
                    ty,
                });
            }
            prefix.pop();
        }
    }
    let mut out = Vec::new();
    let mut prefix = Vec::new();
    walk(struct_id, &mut prefix, get_fields, &mut out);
    out
}

/// 構造体 `struct_id` の `field_index` 番目フィールドが占める葉列の範囲 `(offset, count)` を返す。
pub(crate) fn soa_field_range(
    struct_id: usize,
    field_index: usize,
    get_fields: &impl Fn(usize) -> Vec<Type>,
) -> (usize, usize) {
    fn count_leaves(ty: Type, get_fields: &impl Fn(usize) -> Vec<Type>) -> usize {
        if let Type::Struct(sub_id) = ty {
            get_fields(sub_id)
                .into_iter()
                .map(|t| count_leaves(t, get_fields))
                .sum()
        } else {
            1
        }
    }
    let fields = get_fields(struct_id);
    let mut offset = 0;
    for (i, &ty) in fields.iter().enumerate() {
        let count = count_leaves(ty, get_fields);
        if i == field_index {
            return (offset, count);
        }
        offset += count;
    }
    unreachable!("valid struct field index")
}
