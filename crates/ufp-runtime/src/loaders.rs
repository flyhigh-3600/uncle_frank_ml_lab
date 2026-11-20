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

    /// Parse the metadata and tensor directory of a GGUF v2/v3 file. Tensor
    /// payloads are not decoded or copied; offsets remain file-relative.
    pub fn load_gguf(
        &self,
        relative: impl AsRef<Path>,
        max_bytes: usize,
    ) -> Result<GgufFile, LoadError> {
        let bytes = self.read_bytes(relative, max_bytes)?;
        parse_gguf(&bytes)
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

/// A GGUF tensor-directory entry. `data_offset` is relative to the start of
/// the tensor-data section, as specified by GGUF.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GgufTensor {
    pub name: String,
    pub dimensions: Vec<u64>,
    pub ggml_type: u32,
    pub offset: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GgufFile {
    pub version: u32,
    pub metadata: std::collections::BTreeMap<String, RuntimeValue>,
    pub tensors: Vec<GgufTensor>,
    pub alignment: u32,
    pub tensor_data_offset: u64,
    pub file_size: u64,
}

impl GgufFile {
    /// Produce one root plan and per-layer plans. This exports validated lazy
    /// binary views, not tensor payloads or executable model operations.
    pub fn export_ufp(&self, source_file: &str) -> GgufExport {
        let safe = source_file
            .split('/')
            .filter(|part| !part.is_empty())
            .map(escape_path_part)
            .collect::<Vec<_>>()
            .join("/");
        let mut root = format!("// GGUF v{} metadata and tensor directory\n", self.version);
        for (key, value) in &self.metadata {
            root.push_str(&format!(
                "// metadata {} = {}\n",
                escape_comment(key),
                value_text(value)
            ));
        }
        root.push_str("graph gguf_model {\n");
        let groups = self.layer_groups();
        for (name, _) in &groups {
            root.push_str(&format!(
                "  portal {} from \"layers/{}.ufp\";\n",
                escape_ident(name),
                escape_path_part(name)
            ));
        }
        root.push_str("}\n");
        let layers = groups
            .into_iter()
            .map(|(name, tensors)| {
                let mut text = format!("// GGUF layer {name}\n");
                for tensor in tensors {
                    let dims = tensor
                        .dimensions
                        .iter()
                        .map(u64::to_string)
                        .collect::<Vec<_>>()
                        .join(", ");
                    text.push_str(&format!(
                        "matrix[{}] {} = binary(\"{}\", offset={}, type={});\n",
                        dims,
                        escape_ident(&tensor.name),
                        safe,
                        tensor.offset,
                        tensor.ggml_type
                    ));
                }
                (format!("layers/{}.ufp", escape_path_part(&name)), text)
            })
            .collect();
        GgufExport { root, layers }
    }

    fn layer_groups(&self) -> Vec<(String, Vec<&GgufTensor>)> {
        let mut groups: std::collections::BTreeMap<String, Vec<&GgufTensor>> = Default::default();
        for tensor in &self.tensors {
            let parts = tensor.name.split('.').collect::<Vec<_>>();
            let layer = parts
                .windows(2)
                .find(|pair| pair[0] == "blk")
                .map(|pair| format!("layer_{}", pair[1]))
                .unwrap_or_else(|| "root".into());
            groups.entry(layer).or_default().push(tensor);
        }
        groups.into_iter().collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GgufExport {
    pub root: String,
    pub layers: Vec<(String, String)>,
}

fn escape_path_part(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn escape_ident(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if (c.is_ascii_alphanumeric() || c == '_') && !(i == 0 && c.is_ascii_digit()) {
            out.push(c);
        } else {
            out.push_str(&format!("_{:X}_", c as u32));
        }
    }
    if out.is_empty() {
        "_".into()
    } else {
        out
    }
}
fn escape_comment(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}
fn value_text(v: &RuntimeValue) -> String {
    match v {
        RuntimeValue::Null => "null".into(),
        RuntimeValue::Bool(x) => x.to_string(),
        RuntimeValue::Number(x) => x.to_string(),
        RuntimeValue::String(x) => format!("{x:?}"),
        RuntimeValue::Array(a) => format!(
            "[{}]",
            a.iter().map(value_text).collect::<Vec<_>>().join(", ")
        ),
        RuntimeValue::Object(_) => "{...}".into(),
    }
}

fn parse_gguf(bytes: &[u8]) -> Result<GgufFile, LoadError> {
    let mut c = GgufCursorImpl { b: bytes, p: 0 };
    if c.take(4)? != b"GGUF" {
        return Err(LoadError::Parse("invalid GGUF magic".into()));
    }
    let version = c.u32v()?;
    if !(2..=3).contains(&version) {
        return Err(LoadError::Parse(format!(
            "unsupported GGUF version {version} (supported: 2, 3)"
        )));
    }
    let nt = c.u64v()?;
    let nm = c.u64v()?;
    if nt > 1_000_000 || nm > 1_000_000 {
        return Err(LoadError::Parse(
            "GGUF directory count exceeds limit".into(),
        ));
    }
    let mut metadata = std::collections::BTreeMap::new();
    for _ in 0..nm {
        let key = c.stringv()?;
        let ty = c.u32v()?;
        let value = read_gguf_value(&mut c, ty, 0)?;
        if metadata.insert(key.clone(), value).is_some() {
            return Err(LoadError::Parse(format!(
                "duplicate GGUF metadata key `{key}`"
            )));
        }
    }
    let mut tensors = Vec::with_capacity(nt as usize);
    for _ in 0..nt {
        let name = c.stringv()?;
        let rank = c.u32v()?;
        if rank > 64 {
            return Err(LoadError::Parse("GGUF tensor rank exceeds 64".into()));
        }
        let mut dimensions = Vec::with_capacity(rank as usize);
        for _ in 0..rank {
            dimensions.push(c.u64v()?);
        }
        let ggml_type = c.u32v()?;
        let offset = c.u64v()?;
        tensors.push(GgufTensor {
            name,
            dimensions,
            ggml_type,
            offset,
        });
    }
    let alignment = metadata
        .get("general.alignment")
        .and_then(|v| match v {
            RuntimeValue::Number(n) => n.as_u64(),
            _ => None,
        })
        .unwrap_or(32);
    let alignment =
        u32::try_from(alignment).map_err(|_| LoadError::Parse("invalid GGUF alignment".into()))?;
    if alignment == 0 || !alignment.is_power_of_two() {
        return Err(LoadError::Parse(
            "GGUF alignment must be a nonzero power of two".into(),
        ));
    }
    let data_offset =
        c.p.checked_add(alignment as usize - 1)
            .map(|x| x & !(alignment as usize - 1))
            .ok_or_else(|| LoadError::Parse("GGUF offset overflow".into()))?;
    if data_offset > bytes.len() {
        return Err(LoadError::Parse(
            "GGUF tensor-data section is truncated".into(),
        ));
    }
    let mut names = std::collections::BTreeSet::new();
    for t in &tensors {
        if !names.insert(&t.name) {
            return Err(LoadError::Parse(format!(
                "duplicate GGUF tensor `{}`",
                t.name
            )));
        }
        let absolute = (data_offset as u64)
            .checked_add(t.offset)
            .ok_or_else(|| LoadError::Parse("GGUF tensor offset overflow".into()))?;
        if absolute > bytes.len() as u64 {
            return Err(LoadError::Parse(format!(
                "tensor `{}` offset is outside file",
                t.name
            )));
        }
    }
    Ok(GgufFile {
        version,
        metadata,
        tensors,
        alignment,
        tensor_data_offset: data_offset as u64,
        file_size: bytes.len() as u64,
    })
}

fn read_gguf_value(
    c: &mut impl GgufCursor,
    ty: u32,
    depth: u32,
) -> Result<RuntimeValue, LoadError> {
    if depth > 8 {
        return Err(LoadError::Parse("GGUF array nesting exceeds limit".into()));
    }
    macro_rules! number {
        ($method:ident, $ty:ty) => {{
            let x = c.$method()? as $ty;
            RuntimeValue::Number(serde_json::Number::from(x))
        }};
    }
    Ok(match ty {
        0 => number!(u8v, u64),
        1 => number!(i8v, i64),
        2 => number!(u16v, u64),
        3 => number!(i16v, i64),
        4 => number!(u32v, u64),
        5 => number!(i32v, i64),
        6 => {
            let x = f32::from_bits(c.u32v()?);
            RuntimeValue::Number(
                serde_json::Number::from_f64(x as f64)
                    .ok_or_else(|| LoadError::Parse("non-finite GGUF float".into()))?,
            )
        }
        7 => match c.u8v()? {
            0 => RuntimeValue::Bool(false),
            1 => RuntimeValue::Bool(true),
            _ => return Err(LoadError::Parse("invalid GGUF boolean".into())),
        },
        8 => RuntimeValue::String(c.stringv()?),
        9 => {
            let element = c.u32v()?;
            let n = c.u64v()?;
            if n > 1_000_000 {
                return Err(LoadError::Parse("GGUF array count exceeds limit".into()));
            }
            let mut a = Vec::with_capacity(n as usize);
            for _ in 0..n {
                a.push(read_gguf_value(c, element, depth + 1)?);
            }
            RuntimeValue::Array(a)
        }
        10 => number!(u64v, u64),
        11 => number!(i64v, i64),
        12 => {
            let x = f64::from_bits(c.u64v()?);
            RuntimeValue::Number(
                serde_json::Number::from_f64(x)
                    .ok_or_else(|| LoadError::Parse("non-finite GGUF double".into()))?,
            )
        }
        _ => {
            return Err(LoadError::Parse(format!(
                "unsupported GGUF metadata type {ty}"
            )))
        }
    })
}
trait GgufCursor {
    fn u8v(&mut self) -> Result<u8, LoadError>;
    fn i8v(&mut self) -> Result<i8, LoadError>;
    fn u16v(&mut self) -> Result<u16, LoadError>;
    fn i16v(&mut self) -> Result<i16, LoadError>;
    fn u32v(&mut self) -> Result<u32, LoadError>;
    fn i32v(&mut self) -> Result<i32, LoadError>;
    fn u64v(&mut self) -> Result<u64, LoadError>;
    fn i64v(&mut self) -> Result<i64, LoadError>;
    fn stringv(&mut self) -> Result<String, LoadError>;
}
struct GgufCursorImpl<'a> {
    b: &'a [u8],
    p: usize,
}
impl GgufCursorImpl<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], LoadError> {
        let end = self
            .p
            .checked_add(n)
            .filter(|&x| x <= self.b.len())
            .ok_or_else(|| LoadError::Parse("truncated GGUF".into()))?;
        let value = &self.b[self.p..end];
        self.p = end;
        Ok(value)
    }
}
impl GgufCursor for GgufCursorImpl<'_> {
    fn u8v(&mut self) -> Result<u8, LoadError> {
        Ok(self.take(1)?[0])
    }
    fn i8v(&mut self) -> Result<i8, LoadError> {
        Ok(self.u8v()? as i8)
    }
    fn u16v(&mut self) -> Result<u16, LoadError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i16v(&mut self) -> Result<i16, LoadError> {
        Ok(self.u16v()? as i16)
    }
    fn u32v(&mut self) -> Result<u32, LoadError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32v(&mut self) -> Result<i32, LoadError> {
        Ok(self.u32v()? as i32)
    }
    fn u64v(&mut self) -> Result<u64, LoadError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn i64v(&mut self) -> Result<i64, LoadError> {
        Ok(self.u64v()? as i64)
    }
    fn stringv(&mut self) -> Result<String, LoadError> {
        let n = usize::try_from(self.u64v()?)
            .map_err(|_| LoadError::Parse("GGUF string too large".into()))?;
        String::from_utf8(self.take(n)?.to_vec())
            .map_err(|_| LoadError::Parse("invalid UTF-8 in GGUF string".into()))
    }
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

    fn synthetic_gguf() -> Vec<u8> {
        fn u32b(v: u32, b: &mut Vec<u8>) {
            b.extend(v.to_le_bytes());
        }
        fn u64b(v: u64, b: &mut Vec<u8>) {
            b.extend(v.to_le_bytes());
        }
        fn string(s: &str, b: &mut Vec<u8>) {
            u64b(s.len() as u64, b);
            b.extend(s.as_bytes());
        }
        let mut b = b"GGUF".to_vec();
        u32b(3, &mut b);
        u64b(1, &mut b);
        u64b(2, &mut b);
        string("general.name", &mut b);
        u32b(8, &mut b);
        string("demo", &mut b);
        string("nested.values", &mut b);
        u32b(9, &mut b);
        u32b(4, &mut b);
        u64b(2, &mut b);
        u32b(7, &mut b);
        u32b(9, &mut b);
        string("blk.0.attn.q.weight", &mut b);
        u32b(2, &mut b);
        u64b(4, &mut b);
        u64b(8, &mut b);
        u32b(0, &mut b);
        u64b(0, &mut b);
        b.resize((b.len() + 31) & !31, 0);
        b.extend([0u8; 64]);
        b
    }

    #[test]
    fn parses_sandboxed_gguf_directory_and_exports_safe_lazy_views() {
        let d = std::env::temp_dir().join(format!("ufp-gguf-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("model.gguf"), synthetic_gguf()).unwrap();
        let parsed = ProjectSandbox::new(&d)
            .unwrap()
            .load_gguf("model.gguf", 4096)
            .unwrap();
        assert_eq!(parsed.version, 3);
        assert_eq!(
            parsed.metadata["general.name"],
            RuntimeValue::String("demo".into())
        );
        assert_eq!(
            parsed.metadata["nested.values"],
            RuntimeValue::Array(vec![
                RuntimeValue::Number(serde_json::Number::from(7)),
                RuntimeValue::Number(serde_json::Number::from(9)),
            ])
        );
        assert_eq!(parsed.tensors[0].dimensions, vec![4, 8]);
        let exported = parsed.export_ufp("../bad/model.gguf");
        assert!(exported.root.contains("portal layer_0"));
        assert!(exported.layers[0].1.contains("%2E%2E"));
        assert!(exported.layers[0].1.contains("binary("));
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn rejects_bad_magic_and_unknown_version() {
        assert!(matches!(parse_gguf(b"NOPE"), Err(LoadError::Parse(_))));
        let mut data = synthetic_gguf();
        data[4..8].copy_from_slice(&99u32.to_le_bytes());
        assert!(matches!(parse_gguf(&data), Err(LoadError::Parse(_))));
    }
}
