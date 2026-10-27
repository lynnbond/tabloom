//! Real files are touched only by explicit open/save/export commands.
use crate::model::{FileState, TextEncoding};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("文件读写失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("不是有效的 UTF-8 或带 BOM 的 UTF-16 文本（不支持自动猜测其他编码）")]
    Encoding,
    #[error("文件已被外部修改或删除，未覆盖；请重新打开比较，或导出到另一路径")]
    Conflict,
    #[error("为保护恢复数据，不能将应用数据目录作为普通文本打开或写入")]
    InternalData,
    #[error("旧的文件关联没有可验证的原文件快照，请重新打开或导出到其他路径")]
    MissingBaseline,
}
pub type Result<T> = std::result::Result<T, FileError>;

pub fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub fn decode(bytes: &[u8]) -> Result<(String, TextEncoding)> {
    if bytes.starts_with(&[0xff, 0xfe, 0, 0]) || bytes.starts_with(&[0, 0, 0xfe, 0xff]) {
        return Err(FileError::Encoding);
    }
    let (bytes, encoding) = if let Some(rest) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        (rest, TextEncoding::Utf8Bom)
    } else if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        (rest, TextEncoding::Utf16Le)
    } else if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        (rest, TextEncoding::Utf16Be)
    } else {
        (bytes, TextEncoding::Utf8)
    };
    let text = match encoding {
        TextEncoding::Utf8 | TextEncoding::Utf8Bom => std::str::from_utf8(bytes)
            .map_err(|_| FileError::Encoding)?
            .to_owned(),
        TextEncoding::Utf16Le | TextEncoding::Utf16Be => {
            if !bytes.len().is_multiple_of(2) {
                return Err(FileError::Encoding);
            }
            let units: Vec<_> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| {
                    if encoding == TextEncoding::Utf16Le {
                        u16::from_le_bytes([b[0], b[1]])
                    } else {
                        u16::from_be_bytes([b[0], b[1]])
                    }
                })
                .collect();
            String::from_utf16(&units).map_err(|_| FileError::Encoding)?
        }
    };
    // NUL is a strong signal of binary / BOM-less UTF-16, not a supported text file.
    if text.contains('\0') {
        return Err(FileError::Encoding);
    }
    Ok((text, encoding))
}

pub fn encode(text: &str, encoding: TextEncoding) -> Vec<u8> {
    match encoding {
        TextEncoding::Utf8 => text.as_bytes().to_vec(),
        TextEncoding::Utf8Bom => [b"\xef\xbb\xbf".as_slice(), text.as_bytes()].concat(),
        TextEncoding::Utf16Le | TextEncoding::Utf16Be => {
            let mut out = if encoding == TextEncoding::Utf16Le {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for unit in text.encode_utf16() {
                out.extend(if encoding == TextEncoding::Utf16Le {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
            out
        }
    }
}

pub fn resolve(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(path.canonicalize()?);
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(parent.canonicalize()?.join(
        path.file_name()
            .ok_or_else(|| std::io::Error::other("missing file name"))?,
    ))
}
pub fn guard_path(path: &Path, data_root: Option<&Path>) -> Result<PathBuf> {
    let path = resolve(path)?;
    if let Some(root) = data_root
        && path.starts_with(resolve(root)?)
    {
        return Err(FileError::InternalData);
    }
    Ok(path)
}

pub fn open(path: &Path) -> Result<(String, FileState)> {
    let bytes = fs::read(path)?;
    let (text, encoding) = decode(&bytes)?;
    Ok((
        text,
        FileState {
            encoding,
            disk_hash: hash(&bytes),
        },
    ))
}

fn disk_hash(path: &Path) -> Result<Option<[u8; 32]>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(hash(&bytes))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Stage and sync before atomic replacement; never truncate the original on write failure.
/// The final hash check detects common external edits; it is not a cross-process lock.
fn write_checked(path: &Path, bytes: &[u8], expected: Option<[u8; 32]>) -> Result<()> {
    if disk_hash(path)? != expected {
        return Err(FileError::Conflict);
    }
    if expected == Some(hash(bytes)) {
        return Ok(());
    }
    let mut staged = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))?;
    match fs::metadata(path) {
        Ok(metadata) => staged.as_file().set_permissions(metadata.permissions())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && expected.is_none() => {}
        Err(e) => return Err(e.into()),
    }
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    if disk_hash(path)? != expected {
        return Err(FileError::Conflict);
    }
    staged.persist(path).map_err(|e| FileError::Io(e.error))?;
    Ok(())
}
pub fn save(path: &Path, text: &str, state: &FileState) -> Result<FileState> {
    let bytes = encode(text, state.encoding);
    write_checked(path, &bytes, Some(state.disk_hash))?;
    Ok(FileState {
        encoding: state.encoding,
        disk_hash: hash(&bytes),
    })
}
/// Explicit export writes UTF-8 without BOM and does not change the region's association.
pub fn export(path: &Path, text: &str) -> Result<()> {
    write_checked(path, text.as_bytes(), disk_hash(path)?)
}

pub fn suggested_syntax(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "md" | "markdown" => "markdown",
        "rs" => "rust",
        "go" => "go",
        "py" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" => "typescript",
        "html" | "htm" => "html",
        "css" => "css",
        "sql" => "sql",
        "sh" | "bash" | "zsh" => "shell",
        "ps1" => "powershell",
        "c" | "h" => "c",
        "cpp" | "hpp" | "cc" | "cxx" => "cpp",
        "java" => "java",
        "xml" => "xml",
        _ => "plain",
    }
}
