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
