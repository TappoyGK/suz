//! 配列の要素型・次元数・添字列挙型を共通化する。実行時の長さの値は型に含めない。
use super::{EnumType, Type};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ArrayType {
    pub element: Type,
    pub rank: usize,
    pub index_enum: Option<EnumType>,
}

#[derive(Clone, Default)]
pub(crate) struct Types {
    pub entries: Vec<ArrayType>,
    ids: BTreeMap<ArrayType, usize>,
}
impl Types {
    pub fn intern(&mut self, element: Type, rank: usize) -> Type {
        self.intern_indexed(element, rank, None)
    }
    pub fn intern_indexed(
        &mut self,
        element: Type,
        rank: usize,
        index_enum: Option<EnumType>,
    ) -> Type {
        let array = ArrayType {
            element,
            rank,
            index_enum,
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
}

/// 型の同一性には含めず、仮引数または不変の参照の宣言で束縛する寸法名。
#[derive(Clone)]
pub(crate) struct Dimension {
    pub name: String,
    pub index: usize,
    pub location: super::Location,
}
