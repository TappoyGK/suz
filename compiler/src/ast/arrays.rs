//! 配列の要素型・次元数・次元ごとの添字列挙型を共通化する。長さの次元の値は型に含めない。
use super::{EnumDeclaration, EnumType, Type};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ArrayType {
    pub element: Type,
    pub dims: Vec<Option<EnumType>>,
    pub soa: bool,
}

impl ArrayType {
    pub fn rank(&self) -> usize {
        self.dims.len()
    }

    pub fn first_enum(&self) -> Option<EnumType> {
        self.dims.first().copied().flatten()
    }

    pub fn sub_dims(&self) -> Vec<Option<EnumType>> {
        self.dims[1..].to_vec()
    }

    pub fn runtime_dim_count(&self) -> usize {
        self.dims.iter().filter(|d| d.is_none()).count()
    }

    pub fn runtime_dim_index(&self, axis: usize) -> Option<usize> {
        self.dims[axis]
            .is_none()
            .then(|| self.dims[..axis].iter().filter(|d| d.is_none()).count())
    }

    pub fn format_dims(&self, enums: &[EnumDeclaration]) -> Option<String> {
        if self.dims.len() == 1 && self.dims[0].is_none() {
            return None;
        }
        Some(
            self.dims
                .iter()
                .map(|d| match d {
                    Some(enum_ty) => enums[enum_ty.id].name.as_str(),
                    None => "_",
                })
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

#[derive(Clone, Default)]
pub(crate) struct Types {
    pub entries: Vec<ArrayType>,
    ids: BTreeMap<ArrayType, usize>,
}
impl Types {
    pub fn intern_dims(&mut self, element: Type, dims: Vec<Option<EnumType>>, soa: bool) -> Type {
        let array = ArrayType { element, dims, soa };
        let id = *self.ids.entry(array.clone()).or_insert_with(|| {
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
        self.entries[id].clone()
    }
    pub fn patch_ranges(&mut self, ranges: &[super::RangeDeclaration]) {
        self.ids.clear();
        for (id, entry) in self.entries.iter_mut().enumerate() {
            entry.element.patch_ranges(ranges);
            self.ids.insert(entry.clone(), id);
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
