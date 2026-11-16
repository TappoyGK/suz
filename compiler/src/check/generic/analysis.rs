//! 数値型を記号的に扱って汎用関数の本体を一度検査し、型に依存する失敗を記録する。
//!
//! 利用元: [`crate::check::functions`]。
//! 主な対象規則: `[num.fn-check]`、`[num.fn-unused]`。

use super::super::{functions, scope::Scope};
use crate::ast::Function;

// [num.fn-check]、[num.fn-unused]: 記号的な数値型で一度だけ走査する。
// 各規則が自分の型関数を判定し、診断の共通部分の抽出やエラー回復は行わない。
pub(crate) fn analyze(function: &Function, globals: &Scope) -> super::Analysis {
    let mut scope = globals.clone();
    scope.mode = super::super::scope::Mode::GenericCheck;
    // ランタイムの定数登録に加えるのは、実際に出力する具体化だけ。
    scope.outputs.constant_texts = Default::default();
    let deferred = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    scope.deferred = Some(deferred.clone());
    let result = functions::check_function(function, scope, 0).map(|(_, warnings)| warnings);
    let deferred = if result.is_err() {
        deferred.take()
    } else {
        Vec::new()
    };
    super::Analysis { result, deferred }
}
