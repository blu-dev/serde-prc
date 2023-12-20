use serde::{
    ser::{SerializeMap, SerializeSeq},
    Serialize,
};

use crate::Value;

impl Serialize for Value {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Bool(v) => serializer.serialize_bool(*v),
            Self::I8(v) => serializer.serialize_i8(*v),
            Self::U8(v) => serializer.serialize_u8(*v),
            Self::I16(v) => serializer.serialize_i16(*v),
            Self::U16(v) => serializer.serialize_u16(*v),
            Self::I32(v) => serializer.serialize_i32(*v),
            Self::U32(v) => serializer.serialize_u32(*v),
            Self::F32(v) => serializer.serialize_f32(*v),
            Self::Hash(v) => {
                if serializer.is_human_readable() {
                    v.serialize(serializer)
                } else {
                    v.0.serialize(serializer)
                }
            }
            Self::String(v) => serializer.serialize_str(v),
            Self::List(v) => {
                let mut seq = serializer.serialize_seq(Some(v.len()))?;
                for value in v.iter() {
                    seq.serialize_element(value)?;
                }
                seq.end()
            }
            Self::Map(v) => {
                let is_human = serializer.is_human_readable();
                let mut map = serializer.serialize_map(Some(v.len()))?;
                for (k, v) in v.iter() {
                    if is_human {
                        map.serialize_entry(k, v)?;
                    } else {
                        map.serialize_entry(&k.0, v)?;
                    }
                }
                map.end()
            }
        }
    }
}
