//! 外部Cの探索・リンク引数を検証し、指定順のままネイティブビルドと出力スクリプトへ渡す。
use super::{Failure, Result};
use std::{ffi::OsString, path::Path};

#[derive(Default)]
pub struct ForeignOptions {
    pub args: Vec<OsString>,
}

impl ForeignOptions {
    pub fn push(&mut self, option: &str, value: OsString) -> Result<()> {
        let invalid =
            || Failure::tool(2, "tool.ffi-options", format!("invalid value for {option}"));
        if value.is_empty() || value.as_encoded_bytes().starts_with(b"-") {
            return Err(invalid());
        }
        if option == "--link-library" {
            let name = value.to_str().ok_or_else(invalid)?;
            if !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_+.-".contains(&b))
            {
                return Err(invalid());
            }
            self.args.push(format!("-l{name}").into());
        } else {
            // 存在検査やcanonicalizeはしない。checkとemit-cには外部ファイルを要求しない。
            let path = std::env::current_dir()
                .map_err(|e| Failure::tool(2, "tool.ffi-options", e))?
                .join(Path::new(&value));
            self.args.push(
                match option {
                    "--include-dir" => "-I",
                    "--library-dir" => "-L",
                    // 拡張子によるCソース判定をさせず、指定位置のままリンカーへ渡す。
                    "--link-object" => "-Xlinker",
                    _ => unreachable!(),
                }
                .into(),
            );
            self.args.push(path.into_os_string());
        }
        Ok(())
    }

    /// シェルの再展開を許さず、空白・引用符・非UTF-8を含むパスも引数単位で保持する。
    pub fn script_assignment(&self) -> Vec<u8> {
        use std::os::unix::ffi::OsStrExt;
        let mut output = b"ffi_args=(".to_vec();
        for arg in &self.args {
            output.extend_from_slice(b" '");
            for byte in arg.as_os_str().as_bytes() {
                if *byte == b'\'' {
                    output.extend_from_slice(b"'\\''");
                } else {
                    output.push(*byte);
                }
            }
            output.push(b'\'');
        }
        output.push(b')');
        output
    }
}
