//! Bounded local data loaders. Paths are resolved beneath an explicit sandbox;
//! no loader performs network access or attempts model/GGUF decoding.
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeValue {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<RuntimeValue>),
    Object(std::collections::BTreeMap<String, RuntimeValue>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sheet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    InvalidPath(String),
    TooLarge { limit: usize },
    Parse(String),
}
impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::InvalidPath(p) => write!(f, "path is not within sandbox: {p}"),
            Self::TooLarge { limit } => write!(f, "file exceeds byte limit ({limit})"),
            Self::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}
impl std::error::Error for LoadError {}
impl From<io::Error> for LoadError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// A filesystem root restricting all reads to files beneath its canonical path.
#[derive(Clone, Debug)]
pub struct ProjectSandbox {
    root: PathBuf,
}
impl ProjectSandbox {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, LoadError> {
        Ok(Self {
            root: fs::canonicalize(root)?,
        })
    }
    fn resolve(&self, relative: impl AsRef<Path>) -> Result<PathBuf, LoadError> {
        let p = relative.as_ref();
        if p.as_os_str().is_empty() || p.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(LoadError::InvalidPath(p.display().to_string()));
        }
        let candidate = fs::canonicalize(self.root.join(p))
            .map_err(|_| LoadError::InvalidPath(p.display().to_string()))?;
        if !candidate.starts_with(&self.root) {
            return Err(LoadError::InvalidPath(p.display().to_string()));
        }
        Ok(candidate)
    }
    pub fn read_bytes(
        &self,
        relative: impl AsRef<Path>,
        max_bytes: usize,
    ) -> Result<Vec<u8>, LoadError> {
        let path = self.resolve(relative)?;
        let file = fs::File::open(path)?;
        let mut bytes = Vec::new();
        file.take(max_bytes.saturating_add(1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > max_bytes {
            return Err(LoadError::TooLarge { limit: max_bytes });
        }
        Ok(bytes)
    }
    pub fn load_json(
        &self,
        relative: impl AsRef<Path>,
        max_bytes: usize,
    ) -> Result<RuntimeValue, LoadError> {
        let bytes = self.read_bytes(relative, max_bytes)?;
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| LoadError::Parse(e.to_string()))?;
        Ok(from_json(value))
    }
    pub fn load_csv(
        &self,
        relative: impl AsRef<Path>,
        max_bytes: usize,
    ) -> Result<Sheet, LoadError> {
        let bytes = self.read_bytes(relative, max_bytes)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| LoadError::Parse(e.to_string()))?;
        let mut records = parse_csv(text)?;
        if records.is_empty() {
            return Ok(Sheet {
                columns: vec![],
                rows: vec![],
            });
        }
        let columns = records.remove(0);
        if columns
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != columns.len()
        {
            return Err(LoadError::Parse("duplicate CSV column name".into()));
        }
        if records.iter().any(|r| r.len() != columns.len()) {
            return Err(LoadError::Parse(
                "CSV row width does not match header".into(),
            ));
        }
        Ok(Sheet {
            columns,
            rows: records,
        })
    }
}
use std::io::Read;

fn from_json(v: serde_json::Value) -> RuntimeValue {
    match v {
        serde_json::Value::Null => RuntimeValue::Null,
        serde_json::Value::Bool(v) => RuntimeValue::Bool(v),
        serde_json::Value::Number(v) => RuntimeValue::Number(v),
        serde_json::Value::String(v) => RuntimeValue::String(v),
        serde_json::Value::Array(v) => RuntimeValue::Array(v.into_iter().map(from_json).collect()),
        serde_json::Value::Object(v) => {
            RuntimeValue::Object(v.into_iter().map(|(k, v)| (k, from_json(v))).collect())
        }
    }
}

fn parse_csv(s: &str) -> Result<Vec<Vec<String>>, LoadError> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = s.chars().peekable();
    let mut quoted = false;
    let mut after_quote = false;
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                    after_quote = true;
                }
            } else {
                field.push(c);
            }
        } else if after_quote {
            match c {
                ',' => {
                    row.push(std::mem::take(&mut field));
                    after_quote = false;
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    after_quote = false;
                }
                '\r' if chars.peek() == Some(&'\n') => {
                    chars.next();
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    after_quote = false;
                }
                _ => {
                    return Err(LoadError::Parse(
                        "unexpected character after quoted CSV field".into(),
                    ))
                }
            }
        } else {
            match c {
                '"' if field.is_empty() => quoted = true,
                '"' => return Err(LoadError::Parse("quote inside unquoted CSV field".into())),
                ',' => row.push(std::mem::take(&mut field)),
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                '\r' if chars.peek() == Some(&'\n') => {
                    chars.next();
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                _ => field.push(c),
            }
        }
    }
    if quoted {
        return Err(LoadError::Parse("unterminated quoted CSV field".into()));
    }
    if after_quote || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loads_files_and_rejects_escape_and_size() {
        let d = std::env::temp_dir().join(format!("ufp-loader-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("x.json"), r#"{"nested":[true,3]}"#).unwrap();
        let s = ProjectSandbox::new(&d).unwrap();
        assert!(matches!(
            s.load_json("x.json", 100).unwrap(),
            RuntimeValue::Object(_)
        ));
        assert!(matches!(
            s.read_bytes("../Cargo.toml", 100),
            Err(LoadError::InvalidPath(_))
        ));
        assert!(matches!(
            s.read_bytes("x.json", 2),
            Err(LoadError::TooLarge { .. })
        ));
        fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn parses_quoted_csv_into_sheet() {
        let records = parse_csv("name,value\r\nAda,\"hello, \"\"world\"\"\"\r\n").unwrap();
        assert_eq!(
            records,
            vec![vec!["name", "value"], vec!["Ada", "hello, \"world\""]]
        );
    }
}
