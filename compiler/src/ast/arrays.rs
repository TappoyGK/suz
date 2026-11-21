//! 配列の要素型と次元数を共通化する。寸法の実行時の値は型に含めない。
use super::Type;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ArrayType {
    pub element: Type,
    pub rank: usize,
}

#[derive(Clone, Default)]
pub(crate) struct Types {
    pub entries: Vec<ArrayType>,
    ids: BTreeMap<ArrayType, usize>,
}
impl Types {
    pub fn intern(&mut self, element: Type, rank: usize) -> Type {
        let array = ArrayType { element, rank };
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
