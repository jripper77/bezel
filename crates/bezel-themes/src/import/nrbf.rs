//! A reader for MS-NRBF streams (.NET `BinaryFormatter`) that turns the
//! records into a plain object graph: class instances with named members,
//! strings, arrays and primitives, with references kept as object ids and
//! checked once the whole stream is read (forward references are normal).
//!
//! It reads untrusted files, so it never instantiates anything: it refuses
//! record and type kinds it does not know instead of guessing, accepts only
//! the class names its caller allows, and bounds every length, count and
//! nesting level ([`Limits`]). Zero padding after `MessageEnd` is ignored.

use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// Why a stream was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NrbfError {
    /// Byte offset where the problem was found.
    pub offset: usize,
    /// What is wrong.
    pub reason: String,
}

impl fmt::Display for NrbfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NRBF at byte {}: {}", self.offset, self.reason)
    }
}

type R<T> = Result<T, NrbfError>;

/// Hard limits on what a stream may contain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Largest accepted stream, bytes.
    pub max_input: usize,
    /// Deepest nesting of inline records.
    pub max_depth: usize,
    /// Most objects (records with an id).
    pub max_objects: usize,
    /// Most members of one class.
    pub max_members: usize,
    /// Most member values and array elements in total.
    pub max_values: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_input: 64 * 1024 * 1024,
            max_depth: 32,
            max_objects: 1_000_000,
            max_members: 512,
            max_values: 8_000_000,
        }
    }
}

/// Object ids as written in the stream (negative for inline value types).
pub type ObjectId = i32;

/// A primitive value.
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    /// `System.Boolean`.
    Boolean(bool),
    /// `System.Byte`.
    Byte(u8),
    /// `System.SByte`.
    SByte(i8),
    /// `System.Char`.
    Char(char),
    /// `System.Decimal`, as its invariant text.
    Decimal(String),
    /// `System.Double`.
    Double(f64),
    /// `System.Single`.
    Single(f32),
    /// `System.Int16`.
    Int16(i16),
    /// `System.UInt16`.
    UInt16(u16),
    /// `System.Int32`.
    Int32(i32),
    /// `System.UInt32`.
    UInt32(u32),
    /// `System.Int64`.
    Int64(i64),
    /// `System.UInt64`.
    UInt64(u64),
    /// `System.TimeSpan`, in 100 ns ticks.
    TimeSpan(i64),
    /// `System.DateTime`, raw 64 bits (ticks and kind).
    DateTime(u64),
}

impl Primitive {
    /// The value as a number, when it is one.
    pub fn as_f64(&self) -> Option<f64> {
        let v = match self {
            Primitive::Byte(v) => f64::from(*v),
            Primitive::SByte(v) => f64::from(*v),
            Primitive::Double(v) => *v,
            Primitive::Single(v) => f64::from(*v),
            Primitive::Int16(v) => f64::from(*v),
            Primitive::UInt16(v) => f64::from(*v),
            Primitive::Int32(v) => f64::from(*v),
            Primitive::UInt32(v) => f64::from(*v),
            Primitive::Int64(v) => *v as f64,
            Primitive::UInt64(v) => *v as f64,
            Primitive::Decimal(s) => s.trim().parse().ok()?,
            _ => return None,
        };
        v.is_finite().then_some(v)
    }

    /// The value as an integer, when it is an integer.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Primitive::Byte(v) => Some(i64::from(*v)),
            Primitive::SByte(v) => Some(i64::from(*v)),
            Primitive::Int16(v) => Some(i64::from(*v)),
            Primitive::UInt16(v) => Some(i64::from(*v)),
            Primitive::Int32(v) => Some(i64::from(*v)),
            Primitive::UInt32(v) => Some(i64::from(*v)),
            Primitive::Int64(v) => Some(*v),
            Primitive::UInt64(v) => i64::try_from(*v).ok(),
            _ => None,
        }
    }

    /// The value as a boolean, when it is one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Primitive::Boolean(v) => Some(*v),
            _ => None,
        }
    }
}

/// A member value or an array element.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `null`.
    Null,
    /// A primitive written inline.
    Primitive(Primitive),
    /// Another object of the graph (written inline or referenced).
    Object(ObjectId),
}

/// An instance of a class.
#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    /// Full type name, e.g. `UsbMonitorL.Theme`.
    pub name: String,
    /// Assembly name; `None` for system (mscorlib) classes.
    pub library: Option<String>,
    /// Members in stream order, with their raw names.
    pub members: Vec<(String, Value)>,
}

impl Class {
    /// A member by its short name (see [`split_member_name`]). The class's
    /// own member wins over an inherited one with the same short name.
    pub fn field(&self, name: &str) -> Option<&Value> {
        let mut inherited = None;
        for (raw, value) in &self.members {
            let (base, short) = split_member_name(raw);
            if short == name {
                if base.is_none() {
                    return Some(value);
                }
                inherited.get_or_insert(value);
            }
        }
        inherited
    }
}

/// Splits a serialized member name into the declaring base class (members
/// inherited from a base are written `Base+member`) and the short name, with
/// auto-property backing fields `<x>k__BackingField` shortened to `x`.
pub fn split_member_name(raw: &str) -> (Option<&str>, &str) {
    let (base, rest) = match raw.split_once('+') {
        Some((base, rest)) => (Some(base), rest),
        None => (None, raw),
    };
    let short = rest
        .strip_prefix('<')
        .and_then(|r| r.strip_suffix(">k__BackingField"))
        .unwrap_or(rest);
    (base, short)
}

/// An object of the graph.
#[derive(Debug, Clone, PartialEq)]
pub enum Object {
    /// A class instance.
    Class(Class),
    /// A string.
    String(String),
    /// An array of objects, strings or class instances.
    Array(Vec<Value>),
    /// A byte array (for example the PNG file of a `System.Drawing.Bitmap`).
    Bytes(Vec<u8>),
    /// An array of other primitives.
    Primitives(Vec<Primitive>),
}

/// A parsed stream: every object by id, and the root.
#[derive(Debug, Clone, PartialEq)]
pub struct Graph {
    root: ObjectId,
    objects: HashMap<ObjectId, Object>,
}

impl Graph {
    /// Parses a stream. `allow` decides which class names may appear; any
    /// other class makes the whole stream an error.
    pub fn parse(input: &[u8], limits: &Limits, allow: &dyn Fn(&str) -> bool) -> R<Graph> {
        if input.len() > limits.max_input {
            return Err(NrbfError {
                offset: 0,
                reason: format!(
                    "stream of {} bytes exceeds the {} byte limit",
                    input.len(),
                    limits.max_input
                ),
            });
        }
        let mut parser = Parser::new(input, limits, allow);
        let root = parser.header()?;
        loop {
            let at = parser.pos;
            match parser.record()? {
                Rec::End => break,
                Rec::Value(Value::Object(_)) => {}
                Rec::Value(_) | Rec::Nulls(_) => return Err(parser.err_at(at, "loose value")),
            }
        }
        if input[parser.pos..].iter().any(|b| *b != 0) {
            return Err(parser.err("data after MessageEnd"));
        }
        let graph = Graph {
            root,
            objects: parser.objects,
        };
        graph.check_references()?;
        match graph.objects.get(&root) {
            Some(Object::Class(_)) => Ok(graph),
            _ => Err(NrbfError {
                offset: 0,
                reason: format!("root object {root} is missing or not a class"),
            }),
        }
    }

    /// The root object's id.
    pub fn root(&self) -> ObjectId {
        self.root
    }

    /// An object by id.
    pub fn object(&self, id: ObjectId) -> Option<&Object> {
        self.objects.get(&id)
    }

    /// A class instance by id.
    pub fn class(&self, id: ObjectId) -> Option<&Class> {
        match self.objects.get(&id) {
            Some(Object::Class(c)) => Some(c),
            _ => None,
        }
    }

    /// Number of objects.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// True without objects (never the case for a parsed stream).
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    fn check_references(&self) -> R<()> {
        let values = self
            .objects
            .values()
            .flat_map(|o| -> Box<dyn Iterator<Item = &Value>> {
                match o {
                    Object::Class(c) => Box::new(c.members.iter().map(|(_, v)| v)),
                    Object::Array(items) => Box::new(items.iter()),
                    _ => Box::new(std::iter::empty()),
                }
            });
        for value in values {
            if let Value::Object(id) = value
                && !self.objects.contains_key(id)
            {
                return Err(NrbfError {
                    offset: 0,
                    reason: format!("reference to missing object {id}"),
                });
            }
        }
        Ok(())
    }
}

/// Primitive type codes (`PrimitiveTypeEnumeration`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrimType {
    Boolean,
    Byte,
    Char,
    Decimal,
    Double,
    Int16,
    Int32,
    Int64,
    SByte,
    Single,
    TimeSpan,
    DateTime,
    UInt16,
    UInt32,
    UInt64,
}

impl PrimType {
    fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            1 => PrimType::Boolean,
            2 => PrimType::Byte,
            3 => PrimType::Char,
            5 => PrimType::Decimal,
            6 => PrimType::Double,
            7 => PrimType::Int16,
            8 => PrimType::Int32,
            9 => PrimType::Int64,
            10 => PrimType::SByte,
            11 => PrimType::Single,
            12 => PrimType::TimeSpan,
            13 => PrimType::DateTime,
            14 => PrimType::UInt16,
            15 => PrimType::UInt32,
            16 => PrimType::UInt64,
            _ => return None,
        })
    }

    /// Encoded size, when fixed.
    fn size(self) -> Option<usize> {
        match self {
            PrimType::Boolean | PrimType::Byte | PrimType::SByte => Some(1),
            PrimType::Int16 | PrimType::UInt16 => Some(2),
            PrimType::Int32 | PrimType::UInt32 | PrimType::Single => Some(4),
            PrimType::Int64
            | PrimType::UInt64
            | PrimType::Double
            | PrimType::TimeSpan
            | PrimType::DateTime => Some(8),
            PrimType::Char | PrimType::Decimal => None,
        }
    }
}

/// How a member is encoded: inline primitive, or a full record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberType {
    Primitive(PrimType),
    Record,
}

/// Class metadata, shared by every instance written with `ClassWithId`.
#[derive(Debug)]
struct Meta {
    name: String,
    library: Option<String>,
    members: Vec<String>,
    types: Vec<MemberType>,
}

/// What one record produced.
enum Rec {
    Value(Value),
    Nulls(usize),
    End,
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    limits: &'a Limits,
    allow: &'a dyn Fn(&str) -> bool,
    libraries: HashMap<i32, String>,
    metas: HashMap<ObjectId, Rc<Meta>>,
    objects: HashMap<ObjectId, Object>,
    values: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a [u8], limits: &'a Limits, allow: &'a dyn Fn(&str) -> bool) -> Self {
        Self {
            input,
            pos: 0,
            limits,
            allow,
            libraries: HashMap::new(),
            metas: HashMap::new(),
            objects: HashMap::new(),
            values: 0,
            depth: 0,
        }
    }

    fn err_at(&self, offset: usize, reason: impl Into<String>) -> NrbfError {
        NrbfError {
            offset,
            reason: reason.into(),
        }
    }

    fn err(&self, reason: impl Into<String>) -> NrbfError {
        self.err_at(self.pos, reason)
    }

    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|end| *end <= self.input.len())
            .ok_or_else(|| self.err("unexpected end of stream"))?;
        let bytes = &self.input[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn array<const N: usize>(&mut self) -> R<[u8; N]> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }

    fn i32(&mut self) -> R<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    /// A count or length: an `i32` that must not be negative.
    fn count(&mut self, what: &str) -> R<usize> {
        let at = self.pos;
        let v = self.i32()?;
        usize::try_from(v).map_err(|_| self.err_at(at, format!("negative {what} {v}")))
    }

    /// `LengthPrefixedString`: 7-bit varint length (at most 5 bytes) + UTF-8.
    fn string(&mut self) -> R<String> {
        let at = self.pos;
        let mut len: u64 = 0;
        for i in 0..5 {
            let b = self.u8()?;
            len |= u64::from(b & 0x7f) << (7 * i);
            if b & 0x80 == 0 {
                let len = usize::try_from(len)
                    .ok()
                    .filter(|l| *l <= i32::MAX as usize)
                    .ok_or_else(|| self.err_at(at, "string too long"))?;
                let bytes = self.take(len)?;
                return String::from_utf8(bytes.to_vec())
                    .map_err(|_| self.err_at(at, "string is not UTF-8"));
            }
        }
        Err(self.err_at(at, "bad string length prefix"))
    }

    fn enter(&mut self) -> R<()> {
        self.depth += 1;
        if self.depth > self.limits.max_depth {
            return Err(self.err("records nested too deeply"));
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// Reserves room for `n` more values against the total budget.
    fn spend(&mut self, n: usize) -> R<()> {
        self.values = self
            .values
            .checked_add(n)
            .filter(|v| *v <= self.limits.max_values)
            .ok_or_else(|| self.err("too many values"))?;
        Ok(())
    }

    fn insert(&mut self, at: usize, id: ObjectId, object: Object) -> R<Value> {
        if self.objects.len() >= self.limits.max_objects {
            return Err(self.err_at(at, "too many objects"));
        }
        if self.objects.insert(id, object).is_some() {
            return Err(self.err_at(at, format!("object id {id} defined twice")));
        }
        Ok(Value::Object(id))
    }

    fn header(&mut self) -> R<ObjectId> {
        if self.u8()? != 0 {
            return Err(self.err_at(0, "not an NRBF stream (no header record)"));
        }
        let root = self.i32()?;
        let _header_id = self.i32()?;
        let (major, minor) = (self.i32()?, self.i32()?);
        if (major, minor) != (1, 0) {
            return Err(self.err(format!("unsupported format version {major}.{minor}")));
        }
        Ok(root)
    }

    fn prim_type(&mut self) -> R<PrimType> {
        let at = self.pos;
        let code = self.u8()?;
        PrimType::from_code(code)
            .ok_or_else(|| self.err_at(at, format!("unsupported primitive type {code}")))
    }

    fn primitive(&mut self, t: PrimType) -> R<Primitive> {
        let at = self.pos;
        Ok(match t {
            PrimType::Boolean => match self.u8()? {
                0 => Primitive::Boolean(false),
                1 => Primitive::Boolean(true),
                b => return Err(self.err_at(at, format!("bad boolean {b}"))),
            },
            PrimType::Byte => Primitive::Byte(self.u8()?),
            PrimType::SByte => Primitive::SByte(i8::from_le_bytes(self.array()?)),
            PrimType::Char => Primitive::Char(self.char()?),
            PrimType::Decimal => Primitive::Decimal(self.string()?),
            PrimType::Double => Primitive::Double(f64::from_le_bytes(self.array()?)),
            PrimType::Single => Primitive::Single(f32::from_le_bytes(self.array()?)),
            PrimType::Int16 => Primitive::Int16(i16::from_le_bytes(self.array()?)),
            PrimType::UInt16 => Primitive::UInt16(u16::from_le_bytes(self.array()?)),
            PrimType::Int32 => Primitive::Int32(self.i32()?),
            PrimType::UInt32 => Primitive::UInt32(u32::from_le_bytes(self.array()?)),
            PrimType::Int64 => Primitive::Int64(i64::from_le_bytes(self.array()?)),
            PrimType::UInt64 => Primitive::UInt64(u64::from_le_bytes(self.array()?)),
            PrimType::TimeSpan => Primitive::TimeSpan(i64::from_le_bytes(self.array()?)),
            PrimType::DateTime => Primitive::DateTime(u64::from_le_bytes(self.array()?)),
        })
    }

    /// One UTF-8 encoded character.
    fn char(&mut self) -> R<char> {
        let at = self.pos;
        let first = *self
            .input
            .get(self.pos)
            .ok_or_else(|| self.err("unexpected end of stream"))?;
        let len = match first {
            0x00..=0x7f => 1,
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => return Err(self.err("bad UTF-8 character")),
        };
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes)
            .ok()
            .and_then(|s| s.chars().next())
            .ok_or_else(|| self.err_at(at, "bad UTF-8 character"))
    }

    /// Reads one record, skipping `BinaryLibrary` records in front of it.
    fn record(&mut self) -> R<Rec> {
        loop {
            let at = self.pos;
            let kind = self.u8()?;
            return match kind {
                12 => {
                    self.library(at)?;
                    continue;
                }
                1 => self.class_with_id(at).map(Rec::Value),
                4 => self.class_with_types(at, false).map(Rec::Value),
                5 => self.class_with_types(at, true).map(Rec::Value),
                6 => {
                    let id = self.i32()?;
                    let s = self.string()?;
                    self.insert(at, id, Object::String(s)).map(Rec::Value)
                }
                7 => self.binary_array(at).map(Rec::Value),
                8 => {
                    let t = self.prim_type()?;
                    Ok(Rec::Value(Value::Primitive(self.primitive(t)?)))
                }
                9 => Ok(Rec::Value(Value::Object(self.i32()?))),
                10 => Ok(Rec::Value(Value::Null)),
                11 => Ok(Rec::End),
                13 => {
                    let n = usize::from(self.u8()?);
                    self.nulls(n, at)
                }
                14 => {
                    let n = self.count("null count")?;
                    self.nulls(n, at)
                }
                15 => self.primitive_array(at).map(Rec::Value),
                16 | 17 => {
                    let id = self.i32()?;
                    let len = self.count("array length")?;
                    let items = self.elements(len)?;
                    self.insert(at, id, Object::Array(items)).map(Rec::Value)
                }
                0 => Err(self.err_at(at, "header record in the middle of the stream")),
                2 | 3 => {
                    Err(self.err_at(at, "class records without member types are not supported"))
                }
                21 | 22 => Err(self.err_at(at, "remoting method records are not supported")),
                other => Err(self.err_at(at, format!("unknown record type {other}"))),
            };
        }
    }

    fn nulls(&self, n: usize, at: usize) -> R<Rec> {
        if n == 0 {
            return Err(self.err_at(at, "empty null run"));
        }
        Ok(Rec::Nulls(n))
    }

    fn library(&mut self, at: usize) -> R<()> {
        let id = self.i32()?;
        let name = self.string()?;
        if self.libraries.insert(id, name).is_some() {
            return Err(self.err_at(at, format!("library id {id} defined twice")));
        }
        Ok(())
    }

    /// `MemberTypeInfo`: one `BinaryTypeEnumeration` per member, then the
    /// additional information of those that carry one.
    fn member_types(&mut self, count: usize) -> R<Vec<MemberType>> {
        let kinds = self.take(count)?.to_vec();
        kinds.into_iter().map(|k| self.member_type(k)).collect()
    }

    fn member_type(&mut self, binary_type: u8) -> R<MemberType> {
        Ok(match binary_type {
            0 => MemberType::Primitive(self.prim_type()?),
            1 | 2 | 5 | 6 => MemberType::Record,
            3 => {
                self.string()?;
                MemberType::Record
            }
            4 => {
                self.string()?;
                let at = self.pos;
                let library = self.i32()?;
                if !self.libraries.contains_key(&library) {
                    return Err(self.err_at(at, format!("unknown library id {library}")));
                }
                MemberType::Record
            }
            7 => {
                self.prim_type()?;
                MemberType::Record
            }
            other => return Err(self.err(format!("unknown member type {other}"))),
        })
    }

    fn class_with_types(&mut self, at: usize, with_library: bool) -> R<Value> {
        let id = self.i32()?;
        let name = self.string()?;
        if !(self.allow)(&name) {
            return Err(self.err_at(at, format!("class {name:?} is not allowed")));
        }
        let count = self.count("member count")?;
        if count > self.limits.max_members {
            return Err(self.err_at(at, format!("class {name:?} has {count} members")));
        }
        let members = (0..count).map(|_| self.string()).collect::<R<Vec<_>>>()?;
        let types = self.member_types(count)?;
        let library = if with_library {
            let lib_at = self.pos;
            let lib = self.i32()?;
            let found = self.libraries.get(&lib).cloned();
            Some(found.ok_or_else(|| self.err_at(lib_at, format!("unknown library id {lib}")))?)
        } else {
            None
        };
        let meta = Rc::new(Meta {
            name,
            library,
            members,
            types,
        });
        self.metas.insert(id, Rc::clone(&meta));
        self.instance(at, id, &meta)
    }

    fn class_with_id(&mut self, at: usize) -> R<Value> {
        let id = self.i32()?;
        let meta_id = self.i32()?;
        let meta = self
            .metas
            .get(&meta_id)
            .cloned()
            .ok_or_else(|| self.err_at(at, format!("unknown class metadata {meta_id}")))?;
        self.metas.insert(id, Rc::clone(&meta));
        self.instance(at, id, &meta)
    }

    fn instance(&mut self, at: usize, id: ObjectId, meta: &Meta) -> R<Value> {
        let members = self.members(meta)?;
        let class = Class {
            name: meta.name.clone(),
            library: meta.library.clone(),
            members,
        };
        self.insert(at, id, Object::Class(class))
    }

    fn members(&mut self, meta: &Meta) -> R<Vec<(String, Value)>> {
        self.enter()?;
        self.spend(meta.members.len())?;
        let mut out = Vec::with_capacity(meta.members.len());
        let mut nulls = 0usize;
        for (name, ty) in meta.members.iter().zip(&meta.types) {
            let value = if nulls > 0 {
                nulls -= 1;
                Value::Null
            } else {
                match ty {
                    MemberType::Primitive(t) => Value::Primitive(self.primitive(*t)?),
                    MemberType::Record => match self.record()? {
                        Rec::Value(v) => v,
                        Rec::Nulls(n) => {
                            nulls = n - 1;
                            Value::Null
                        }
                        Rec::End => return Err(self.err("MessageEnd inside an object")),
                    },
                }
            };
            out.push((name.clone(), value));
        }
        if nulls > 0 {
            return Err(self.err("null run longer than the remaining members"));
        }
        self.leave();
        Ok(out)
    }

    fn elements(&mut self, len: usize) -> R<Vec<Value>> {
        self.enter()?;
        self.spend(len)?;
        let mut out = Vec::with_capacity(len.min(4096));
        while out.len() < len {
            match self.record()? {
                Rec::Value(v) => out.push(v),
                Rec::Nulls(n) if n <= len - out.len() => {
                    out.extend(std::iter::repeat_n(Value::Null, n));
                }
                Rec::Nulls(_) => return Err(self.err("null run longer than the array")),
                Rec::End => return Err(self.err("MessageEnd inside an array")),
            }
        }
        self.leave();
        Ok(out)
    }

    fn primitives(&mut self, t: PrimType, len: usize) -> R<Object> {
        if let Some(size) = t.size()
            && len.saturating_mul(size) > self.input.len() - self.pos
        {
            return Err(self.err("array longer than the stream"));
        }
        if t == PrimType::Byte {
            return Ok(Object::Bytes(self.take(len)?.to_vec()));
        }
        self.spend(len)?;
        let mut out = Vec::with_capacity(len.min(4096));
        for _ in 0..len {
            out.push(self.primitive(t)?);
        }
        Ok(Object::Primitives(out))
    }

    fn primitive_array(&mut self, at: usize) -> R<Value> {
        let id = self.i32()?;
        let len = self.count("array length")?;
        let t = self.prim_type()?;
        let object = self.primitives(t, len)?;
        self.insert(at, id, object)
    }

    /// `BinaryArray`: only single-dimension, zero-based arrays are accepted.
    fn binary_array(&mut self, at: usize) -> R<Value> {
        let id = self.i32()?;
        let shape = self.u8()?;
        if shape != 0 {
            return Err(self.err_at(at, format!("unsupported array shape {shape}")));
        }
        let rank = self.i32()?;
        if rank != 1 {
            return Err(self.err_at(at, format!("unsupported array rank {rank}")));
        }
        let len = self.count("array length")?;
        let kind = self.u8()?;
        let object = match self.member_type(kind)? {
            MemberType::Primitive(t) => self.primitives(t, len)?,
            MemberType::Record => Object::Array(self.elements(len)?),
        };
        self.insert(at, id, object)
    }
}

/// A minimal NRBF writer used by the tests of this module and of the
/// importers to build synthetic streams by hand.
#[cfg(test)]
pub(crate) mod write {
    /// Bytes of a stream under construction.
    #[derive(Default)]
    pub struct Stream(pub Vec<u8>);

    /// A member type for class records.
    #[derive(Clone, Copy)]
    pub enum Ty<'a> {
        /// Inline primitive with its type code.
        Prim(u8),
        /// A string record.
        Str,
        /// Any object record.
        Obj,
        /// A system class (name).
        System(&'a str),
        /// A class of a library (name, library id).
        Class(&'a str, i32),
        /// An object array.
        ObjArray,
        /// A string array.
        StrArray,
        /// A primitive array (type code).
        PrimArray(u8),
    }

    impl Stream {
        pub fn u8(&mut self, v: u8) -> &mut Self {
            self.0.push(v);
            self
        }
        pub fn i32(&mut self, v: i32) -> &mut Self {
            self.0.extend_from_slice(&v.to_le_bytes());
            self
        }
        pub fn raw(&mut self, v: &[u8]) -> &mut Self {
            self.0.extend_from_slice(v);
            self
        }
        pub fn lps(&mut self, s: &str) -> &mut Self {
            let mut n = s.len();
            loop {
                let b = (n & 0x7f) as u8;
                n >>= 7;
                if n == 0 {
                    self.0.push(b);
                    break;
                }
                self.0.push(b | 0x80);
            }
            self.raw(s.as_bytes())
        }
        pub fn header(&mut self, root: i32) -> &mut Self {
            self.u8(0).i32(root).i32(-1).i32(1).i32(0)
        }
        pub fn library(&mut self, id: i32, name: &str) -> &mut Self {
            self.u8(12).i32(id).lps(name)
        }
        /// `ClassWithMembersAndTypes` (library `Some`) or
        /// `SystemClassWithMembersAndTypes` (`None`) header; the caller
        /// writes the member values next.
        pub fn class(
            &mut self,
            id: i32,
            name: &str,
            members: &[(&str, Ty<'_>)],
            library: Option<i32>,
        ) -> &mut Self {
            self.u8(if library.is_some() { 5 } else { 4 })
                .i32(id)
                .lps(name)
                .i32(members.len() as i32);
            for (m, _) in members {
                self.lps(m);
            }
            for (_, t) in members {
                self.u8(match t {
                    Ty::Prim(_) => 0,
                    Ty::Str => 1,
                    Ty::Obj => 2,
                    Ty::System(_) => 3,
                    Ty::Class(..) => 4,
                    Ty::ObjArray => 5,
                    Ty::StrArray => 6,
                    Ty::PrimArray(_) => 7,
                });
            }
            for (_, t) in members {
                match t {
                    Ty::Prim(p) | Ty::PrimArray(p) => {
                        self.u8(*p);
                    }
                    Ty::System(n) => {
                        self.lps(n);
                    }
                    Ty::Class(n, lib) => {
                        self.lps(n).i32(*lib);
                    }
                    _ => {}
                }
            }
            if let Some(lib) = library {
                self.i32(lib);
            }
            self
        }
        pub fn with_id(&mut self, id: i32, meta: i32) -> &mut Self {
            self.u8(1).i32(id).i32(meta)
        }
        pub fn string(&mut self, id: i32, s: &str) -> &mut Self {
            self.u8(6).i32(id).lps(s)
        }
        pub fn reference(&mut self, id: i32) -> &mut Self {
            self.u8(9).i32(id)
        }
        pub fn null(&mut self) -> &mut Self {
            self.u8(10)
        }
        pub fn nulls(&mut self, n: u8) -> &mut Self {
            self.u8(13).u8(n)
        }
        pub fn bytes(&mut self, id: i32, data: &[u8]) -> &mut Self {
            self.u8(15).i32(id).i32(data.len() as i32).u8(2).raw(data)
        }
        pub fn end(&mut self) -> &mut Self {
            self.u8(11)
        }
    }
}

#[cfg(test)]
#[allow(clippy::panic)] // a failing test panics
mod tests {
    use super::write::{Stream, Ty};
    use super::*;

    fn any(_: &str) -> bool {
        true
    }

    fn parse(bytes: &[u8]) -> R<Graph> {
        Graph::parse(bytes, &Limits::default(), &any)
    }

    fn reason(bytes: &[u8]) -> String {
        parse(bytes).expect_err("refused").reason
    }

    /// A root class with a forward reference to a string, an inline value
    /// type, a shared metadata instance, a byte array and a padded tail.
    fn sample() -> Vec<u8> {
        let mut s = Stream::default();
        s.header(1).library(2, "Lib, Version=1.0");
        s.class(
            1,
            "Lib.Root",
            &[
                ("<name>k__BackingField", Ty::Str),
                ("count", Ty::Prim(8)),
                ("ok", Ty::Prim(1)),
                ("color", Ty::Class("Lib.Color", 2)),
                ("other", Ty::Class("Lib.Color", 2)),
                ("pic", Ty::PrimArray(2)),
                ("Base+count", Ty::Prim(8)),
                ("Base+extra", Ty::Prim(6)),
                ("a", Ty::Obj),
                ("b", Ty::Obj),
            ],
            Some(2),
        );
        s.reference(10).i32(7).u8(1);
        s.class(-5, "Lib.Color", &[("value", Ty::Prim(9))], Some(2))
            .raw(&0xff11_2233_i64.to_le_bytes());
        s.with_id(-6, -5).raw(&7_i64.to_le_bytes());
        s.reference(11).i32(9).raw(&2.5_f64.to_le_bytes());
        s.nulls(2);
        s.string(10, "hello").bytes(11, &[1, 2, 3]).end();
        s.raw(&[0, 0, 0]);
        s.0
    }

    #[test]
    fn parses_objects_references_and_padding() {
        let g = parse(&sample()).expect("parses");
        assert_eq!(g.root(), 1);
        assert_eq!(g.len(), 5);
        assert!(!g.is_empty());
        let root = g.class(1).expect("root");
        assert_eq!(root.name, "Lib.Root");
        assert_eq!(root.library.as_deref(), Some("Lib, Version=1.0"));
        let Some(Value::Object(name)) = root.field("name") else {
            panic!("name")
        };
        assert_eq!(g.object(*name), Some(&Object::String("hello".into())));
        assert_eq!(
            root.field("count"),
            Some(&Value::Primitive(Primitive::Int32(7))),
            "own member wins over the inherited one"
        );
        assert_eq!(
            root.field("extra"),
            Some(&Value::Primitive(Primitive::Double(2.5)))
        );
        assert_eq!(root.field("a"), Some(&Value::Null));
        assert_eq!(root.field("b"), Some(&Value::Null));
        assert_eq!(root.field("missing"), None);
        let Some(Value::Object(other)) = root.field("other") else {
            panic!("other")
        };
        let other = g.class(*other).expect("class");
        assert_eq!(other.name, "Lib.Color");
        assert_eq!(
            other.field("value"),
            Some(&Value::Primitive(Primitive::Int64(7)))
        );
        let Some(Value::Object(pic)) = root.field("pic") else {
            panic!("pic")
        };
        assert_eq!(g.object(*pic), Some(&Object::Bytes(vec![1, 2, 3])));
        assert!(g.class(*pic).is_none());
    }

    #[test]
    fn member_names_are_shortened() {
        assert_eq!(split_member_name("<hide>k__BackingField"), (None, "hide"));
        assert_eq!(
            split_member_name("GraphItem+<posX>k__BackingField"),
            (Some("GraphItem"), "posX")
        );
        assert_eq!(
            split_member_name("Collection`1+items"),
            (Some("Collection`1"), "items")
        );
        assert_eq!(split_member_name("_size"), (None, "_size"));
    }

    #[test]
    fn primitives_convert() {
        use Primitive::*;
        let cases = [
            (Byte(3), Some(3.0), Some(3)),
            (SByte(-3), Some(-3.0), Some(-3)),
            (Int16(-4), Some(-4.0), Some(-4)),
            (UInt16(4), Some(4.0), Some(4)),
            (Int32(5), Some(5.0), Some(5)),
            (UInt32(5), Some(5.0), Some(5)),
            (Int64(6), Some(6.0), Some(6)),
            (UInt64(6), Some(6.0), Some(6)),
            (UInt64(u64::MAX), Some(u64::MAX as f64), None),
            (Single(1.5), Some(1.5), None),
            (Double(f64::NAN), None, None),
            (Decimal(" 2.25".into()), Some(2.25), None),
            (Boolean(true), None, None),
            (Char('x'), None, None),
            (TimeSpan(1), None, None),
            (DateTime(1), None, None),
        ];
        for (p, f, i) in cases {
            assert_eq!(p.as_f64(), f, "{p:?}");
            assert_eq!(p.as_i64(), i, "{p:?}");
        }
        assert_eq!(Boolean(false).as_bool(), Some(false));
        assert_eq!(Int32(1).as_bool(), None);
    }

    /// Every primitive type inline, a typed member primitive, a string
    /// array, a primitive array and a `BinaryArray` of classes.
    #[test]
    fn reads_every_primitive_and_array_kind() {
        let mut s = Stream::default();
        s.header(1);
        let prims: Vec<(String, u8)> = [1u8, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
            .iter()
            .map(|c| (format!("p{c}"), *c))
            .collect();
        let mut members: Vec<(&str, Ty<'_>)> = prims
            .iter()
            .map(|(n, c)| (n.as_str(), Ty::Prim(*c)))
            .collect();
        members.extend([
            ("boxed", Ty::Obj),
            ("strings", Ty::StrArray),
            ("ints", Ty::PrimArray(8)),
            ("items", Ty::ObjArray),
            ("sys", Ty::System("System.Thing")),
        ]);
        s.class(1, "Root", &members, None);
        s.u8(1).u8(0xff).raw("é".as_bytes()).lps("1.5");
        s.raw(&1.25_f64.to_le_bytes()).raw(&(-2_i16).to_le_bytes());
        s.i32(-3).raw(&(-4_i64).to_le_bytes()).u8(0xfe);
        s.raw(&0.5_f32.to_le_bytes()).raw(&5_i64.to_le_bytes());
        s.raw(&6_u64.to_le_bytes()).raw(&7_u16.to_le_bytes());
        s.raw(&8_u32.to_le_bytes()).raw(&9_u64.to_le_bytes());
        s.u8(8).u8(8).i32(42);
        s.u8(17).i32(20).i32(3).string(21, "a").null().reference(21);
        s.u8(15).i32(22).i32(2).u8(8).i32(1).i32(2);
        // BinaryArray: single, rank 1, length 3, element type Class.
        s.library(2, "Lib");
        s.u8(7)
            .i32(23)
            .u8(0)
            .i32(1)
            .i32(3)
            .u8(4)
            .lps("Lib.Item")
            .i32(2);
        s.class(24, "Lib.Item", &[("v", Ty::Prim(8))], Some(2))
            .i32(1);
        s.with_id(25, 24).i32(2);
        s.u8(14).i32(1);
        s.u8(16).i32(26).i32(0);
        s.end();
        let g = parse(&s.0).expect("parses");
        let root = g.class(1).expect("root");
        let p = |n: &str| match root.field(n) {
            Some(Value::Primitive(p)) => p.clone(),
            other => panic!("{n}: {other:?}"),
        };
        assert_eq!(p("p1"), Primitive::Boolean(true));
        assert_eq!(p("p2"), Primitive::Byte(0xff));
        assert_eq!(p("p3"), Primitive::Char('é'));
        assert_eq!(p("p5"), Primitive::Decimal("1.5".into()));
        assert_eq!(p("p6"), Primitive::Double(1.25));
        assert_eq!(p("p7"), Primitive::Int16(-2));
        assert_eq!(p("p8"), Primitive::Int32(-3));
        assert_eq!(p("p9"), Primitive::Int64(-4));
        assert_eq!(p("p10"), Primitive::SByte(-2));
        assert_eq!(p("p11"), Primitive::Single(0.5));
        assert_eq!(p("p12"), Primitive::TimeSpan(5));
        assert_eq!(p("p13"), Primitive::DateTime(6));
        assert_eq!(p("p14"), Primitive::UInt16(7));
        assert_eq!(p("p15"), Primitive::UInt32(8));
        assert_eq!(p("p16"), Primitive::UInt64(9));
        assert_eq!(p("boxed"), Primitive::Int32(42));
        assert_eq!(
            g.object(20),
            Some(&Object::Array(vec![
                Value::Object(21),
                Value::Null,
                Value::Object(21)
            ]))
        );
        assert_eq!(
            g.object(22),
            Some(&Object::Primitives(vec![
                Primitive::Int32(1),
                Primitive::Int32(2)
            ]))
        );
        assert_eq!(
            g.object(23),
            Some(&Object::Array(vec![
                Value::Object(24),
                Value::Object(25),
                Value::Null
            ]))
        );
        assert_eq!(g.class(25).map(|c| c.name.as_str()), Some("Lib.Item"));
        assert_eq!(g.object(26), Some(&Object::Array(vec![])));
    }

    fn root_with(member: Ty<'_>, value: &[u8]) -> Vec<u8> {
        let mut s = Stream::default();
        s.header(1)
            .class(1, "Root", &[("m", member)], None)
            .raw(value);
        s.end();
        s.0
    }

    #[test]
    fn refuses_malformed_streams() {
        assert!(reason(&[]).contains("unexpected end"));
        assert!(reason(&[1, 0, 0]).contains("no header"));
        let mut s = Stream::default();
        s.u8(0).i32(1).i32(-1).i32(2).i32(0);
        assert!(reason(&s.0).contains("version 2.0"));
        let mut good = sample();
        good.push(1);
        assert!(reason(&good).contains("after MessageEnd"));
        let truncated = sample();
        assert!(reason(&truncated[..truncated.len() - 12]).contains("unexpected end"));
        for (record, why) in [
            (0u8, "header record"),
            (2, "without member types"),
            (3, "without member types"),
            (21, "remoting"),
            (22, "remoting"),
            (99, "unknown record type 99"),
        ] {
            let mut s = Stream::default();
            s.header(1).u8(record);
            assert!(reason(&s.0).contains(why), "{record}");
        }
        assert!(reason(&root_with(Ty::Prim(4), &[])).contains("primitive type 4"));
        assert!(reason(&root_with(Ty::Prim(1), &[2])).contains("bad boolean"));
        assert!(reason(&root_with(Ty::Prim(3), &[0xff])).contains("UTF-8"));
        assert!(reason(&root_with(Ty::Prim(3), &[0xc3, 0x28])).contains("UTF-8"));
        assert!(reason(&root_with(Ty::Prim(5), &[0x80; 6])).contains("length prefix"));
        assert!(reason(&root_with(Ty::Prim(5), &[2, 0xff, 0xfe])).contains("not UTF-8"));
        assert!(reason(&root_with(Ty::Obj, &[11])).contains("MessageEnd inside"));
        assert!(reason(&root_with(Ty::Obj, &[13, 2])).contains("null run longer"));
        assert!(reason(&root_with(Ty::Obj, &[13, 0])).contains("empty null run"));
        assert!(reason(&root_with(Ty::Obj, &[9, 7, 0, 0, 0])).contains("missing object 7"));
        let mut s = Stream::default();
        s.header(1)
            .class(1, "Root", &[("m", Ty::Class("X", 9))], None);
        assert!(reason(&s.0).contains("unknown library id 9"));
        let mut s = Stream::default();
        s.header(1).class(1, "Root", &[], Some(9));
        assert!(reason(&s.0).contains("unknown library id 9"));
        let mut s = Stream::default();
        s.header(1).u8(4).i32(1).lps("Root").i32(1).lps("m").u8(9);
        assert!(reason(&s.0).contains("unknown member type 9"));
        let mut s = Stream::default();
        s.header(1).with_id(1, 5);
        assert!(reason(&s.0).contains("unknown class metadata 5"));
        let mut s = Stream::default();
        s.header(1).u8(4).i32(1).lps("Root").i32(-1);
        assert!(reason(&s.0).contains("negative member count"));
    }

    #[test]
    fn refuses_bad_identities_and_arrays() {
        let mut s = Stream::default();
        s.header(1).string(1, "a").string(1, "b").end();
        assert!(reason(&s.0).contains("defined twice"));
        let mut s = Stream::default();
        s.header(1).library(2, "a").library(2, "b");
        assert!(reason(&s.0).contains("library id 2 defined twice"));
        let mut s = Stream::default();
        s.header(1).string(1, "not a class").end();
        assert!(reason(&s.0).contains("not a class"));
        let mut s = Stream::default();
        s.header(1).null().end();
        assert!(reason(&s.0).contains("loose value"));
        let mut s = Stream::default();
        s.header(1).u8(7).i32(2).u8(2);
        assert!(reason(&s.0).contains("array shape 2"));
        let mut s = Stream::default();
        s.header(1).u8(7).i32(2).u8(0).i32(2);
        assert!(reason(&s.0).contains("array rank 2"));
        let mut s = Stream::default();
        s.header(1).u8(15).i32(2).i32(1000).u8(8);
        assert!(reason(&s.0).contains("longer than the stream"));
        let mut s = Stream::default();
        s.header(1).u8(16).i32(2).i32(2).nulls(3);
        assert!(reason(&s.0).contains("longer than the array"));
        let mut s = Stream::default();
        s.header(1).u8(16).i32(2).i32(2).end();
        assert!(reason(&s.0).contains("MessageEnd inside an array"));
        let mut s = Stream::default();
        s.header(1).u8(16).i32(2).i32(-1);
        assert!(reason(&s.0).contains("negative array length"));
        let mut s = Stream::default();
        s.header(1).u8(14).i32(-2);
        assert!(reason(&s.0).contains("negative null count"));
    }

    #[test]
    fn enforces_limits_and_the_class_whitelist() {
        let only_root = |name: &str| name == "Root";
        let mut s = Stream::default();
        s.header(1).class(1, "Evil.Gadget", &[], None).end();
        let e = Graph::parse(&s.0, &Limits::default(), &only_root).expect_err("refused");
        assert!(e.reason.contains("\"Evil.Gadget\" is not allowed"), "{e}");
        assert!(e.to_string().starts_with("NRBF at byte 17"));

        let tight = |f: fn(&mut Limits)| {
            let mut l = Limits::default();
            f(&mut l);
            l
        };
        let deep = {
            let mut s = Stream::default();
            s.header(1).class(1, "Root", &[("m", Ty::Obj)], None);
            s.class(2, "Root", &[("m", Ty::Obj)], None).null().end();
            s.0
        };
        let limits = tight(|l| l.max_depth = 1);
        assert!(Graph::parse(&deep, &limits, &any).is_err());
        let limits = tight(|l| l.max_input = 10);
        assert!(Graph::parse(&sample(), &limits, &any).is_err());
        let limits = tight(|l| l.max_objects = 2);
        assert!(Graph::parse(&sample(), &limits, &any).is_err());
        let limits = tight(|l| l.max_values = 3);
        assert!(Graph::parse(&sample(), &limits, &any).is_err());
        let limits = tight(|l| l.max_members = 2);
        assert!(Graph::parse(&sample(), &limits, &any).is_err());
        let mut s = Stream::default();
        s.header(1).u8(16).i32(2).i32(i32::MAX);
        assert!(reason(&s.0).contains("too many values"));
    }
}
