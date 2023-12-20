use std::fmt::Debug;

use byteorder::{LittleEndian, ReadBytesExt};
pub use hash40::Hash40;
use indexmap::IndexMap;
use serde::Deserialize;

use crate::de::{ReferenceData, ValueDeserializer};
pub mod de;
pub mod ser;

pub use ser::to_vec;

#[cfg(test)]
mod tests;

macro_rules! decl_id {
    ($($name:ident => ($value:expr, $t:path)),*) => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        #[repr(u8)]
        pub enum ParamId {
            $($name = $value,)*
        }

        #[derive(Clone, PartialEq)]
        pub enum Value {
            $(
                $name($t),
            )*
        }

        $(
            impl From<$t> for Value {
                fn from(value: $t) -> Self {
                    Self::$name(value)
                }
            }
        )*


        impl TryFrom<u8> for ParamId {
            type Error = u8;

            fn try_from(value: u8) -> Result<Self, Self::Error> {
                match value {
                    $(
                        $value => Ok(Self::$name),
                    )*
                    other => Err(other)
                }
            }
        }

        impl From<ParamId> for u8 {
            fn from(value: ParamId) -> Self {
                value as u8
            }
        }
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(v) => Debug::fmt(v, f),
            Self::I8(v) => Debug::fmt(v, f),
            Self::U8(v) => Debug::fmt(v, f),
            Self::I16(v) => Debug::fmt(v, f),
            Self::U16(v) => Debug::fmt(v, f),
            Self::I32(v) => Debug::fmt(v, f),
            Self::U32(v) => Debug::fmt(v, f),
            Self::F32(v) => Debug::fmt(v, f),
            Self::Hash(v) => write!(f, "{v}"),
            Self::String(v) => Debug::fmt(v, f),
            Self::List(v) => Debug::fmt(v, f),
            Self::Map(v) => {
                let mut map = f.debug_map();
                for (k, v) in v.iter() {
                    map.key(&format!("{k}"));
                    map.value(v);
                }
                map.finish()
            }
        }
    }
}

decl_id! {
    Bool => (1, bool),
    I8 => (2, i8),
    U8 => (3, u8),
    I16 => (4, i16),
    U16 => (5, u16),
    I32 => (6, i32),
    U32 => (7, u32),
    F32 => (8, f32),
    Hash => (9, Hash40),
    String => (10, String),
    List => (11, Vec<Value>),
    Map => (12, IndexMap<Hash40, Value>)
}

impl Value {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_i8(&self) -> Option<i8> {
        match self {
            Self::I8(v) => Some(*v),
            Self::U8(v) => (*v).try_into().ok(),
            Self::I16(v) => (*v).try_into().ok(),
            Self::U16(v) => (*v).try_into().ok(),
            Self::I32(v) => (*v).try_into().ok(),
            Self::U32(v) => (*v).try_into().ok(),
            _ => None,
        }
    }

    pub fn as_u8(&self) -> Option<u8> {
        match self {
            Self::U8(v) => Some(*v),
            Self::I8(v) => (*v).try_into().ok(),
            Self::I16(v) => (*v).try_into().ok(),
            Self::U16(v) => (*v).try_into().ok(),
            Self::I32(v) => (*v).try_into().ok(),
            Self::U32(v) => (*v).try_into().ok(),
            _ => None,
        }
    }

    pub fn as_i16(&self) -> Option<i16> {
        match self {
            Self::I8(v) => Some(*v as i16),
            Self::U8(v) => Some(*v as i16),
            Self::I16(v) => Some(*v),
            Self::U16(v) => (*v).try_into().ok(),
            Self::I32(v) => (*v).try_into().ok(),
            Self::U32(v) => (*v).try_into().ok(),
            _ => None,
        }
    }

    pub fn as_u16(&self) -> Option<u16> {
        match self {
            Self::I8(v) => Some(*v as u16),
            Self::U8(v) => Some(*v as u16),
            Self::U16(v) => Some(*v),
            Self::I16(v) => (*v).try_into().ok(),
            Self::I32(v) => (*v).try_into().ok(),
            Self::U32(v) => (*v).try_into().ok(),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Self::I8(v) => Some(*v as i32),
            Self::U8(v) => Some(*v as i32),
            Self::I16(v) => Some(*v as i32),
            Self::U16(v) => Some(*v as i32),
            Self::I32(v) => Some(*v),
            Self::U32(v) => (*v).try_into().ok(),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::I8(v) => Some(*v as u32),
            Self::U8(v) => Some(*v as u32),
            Self::I16(v) => Some(*v as u32),
            Self::U16(v) => Some(*v as u32),
            Self::U32(v) => Some(*v),
            Self::I32(v) => (*v).try_into().ok(),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Self::F32(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_hash(&self) -> Option<Hash40> {
        match self {
            Self::Hash(hash) => Some(*hash),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Self::List(list) => Some(list),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&IndexMap<Hash40, Value>> {
        match self {
            Self::Map(map) => Some(map),
            _ => None,
        }
    }

    pub fn merge(&mut self, other: &Value) {
        match self {
            Self::Bool(v) => {
                if let Some(other) = other.as_bool() {
                    *v = other;
                }
            }
            Self::I8(v) => {
                if let Some(other) = other.as_i8() {
                    *v = other;
                }
            }
            Self::U8(v) => {
                if let Some(other) = other.as_u8() {
                    *v = other;
                }
            }
            Self::I16(v) => {
                if let Some(other) = other.as_i16() {
                    *v = other;
                }
            }
            Self::U16(v) => {
                if let Some(other) = other.as_u16() {
                    *v = other;
                }
            }
            Self::I32(v) => {
                if let Some(other) = other.as_i32() {
                    *v = other;
                }
            }
            Self::U32(v) => {
                if let Some(other) = other.as_u32() {
                    *v = other;
                }
            }
            Self::F32(v) => {
                if let Some(other) = other.as_f32() {
                    *v = other;
                }
            }
            Self::Hash(v) => {
                if let Some(other) = other.as_hash() {
                    *v = other;
                }
            }
            Self::String(v) => {
                if let Some(other) = other.as_str() {
                    *v = other.to_string();
                }
            }
            Self::List(v) => {
                if let Some(other) = other.as_list() {
                    v.iter_mut().zip(other).for_each(|(v, other)| {
                        v.merge(other);
                    });
                }
            }
            Self::Map(v) => {
                if let Some(other) = other.as_map() {
                    for (k, v) in v.iter_mut() {
                        if let Some(other) = other.get(k) {
                            v.merge(other);
                        }
                    }
                }
            }
        }
    }
}

pub fn from_reader<T: for<'de> Deserialize<'de>, R: std::io::Read + std::io::Seek>(
    mut reader: R,
) -> Result<T, de::Error> {
    // Check magic
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic).unwrap();

    assert_eq!(magic, *b"paracobn");

    let hash_data_size = reader.read_u32::<LittleEndian>().unwrap();
    assert_eq!(hash_data_size % 8, 0);
    let ref_data_size = reader.read_u32::<LittleEndian>().unwrap();

    let hashes: Vec<_> = (0..hash_data_size / 8)
        .map(|_| Hash40(reader.read_u64::<LittleEndian>().unwrap()))
        .collect();

    let mut ref_data = Vec::with_capacity(ref_data_size as usize);
    unsafe {
        ref_data.set_len(ref_data_size as usize);
        reader.read_exact(&mut ref_data).unwrap();
    }

    let mut deserializer = ValueDeserializer::new(
        ReferenceData::new(ref_data, 8 + hash_data_size as usize),
        &hashes,
        &mut reader,
    );

    T::deserialize(&mut deserializer)
}

pub fn from_slice<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, de::Error> {
    from_reader(std::io::Cursor::new(bytes))
}
