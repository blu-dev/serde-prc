use byteorder::{ByteOrder, LittleEndian, ReadBytesExt};
use hash40::Hash40;
use indexmap::IndexMap;
use serde::{
    de::{MapAccess, SeqAccess, Visitor},
    forward_to_deserialize_any, Deserialize, Deserializer,
};
use std::{
    collections::HashMap,
    fmt::{Debug, Display},
    io::{Read, Seek, SeekFrom},
    task::Wake,
};
use thiserror::Error;

use crate::{ParamId, Value};

#[derive(Debug)]
enum ParseId {
    ParamId,
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    F32,
    Hash,
    String,
    List,
    Map,
}

pub struct Error {
    cause: ErrorKind,
    position_stack: Vec<(ParseId, Option<u64>)>,
}

impl Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.cause)?;
        for (id, position) in self.position_stack.iter() {
            if let Some(position) = position {
                write!(f, "\nwhile parsing {id:?} @ {position:#x}")?;
            } else {
                write!(f, "\nwhile parsing {id:?} @ <unknown>")?;
            }
        }

        Ok(())
    }
}

impl std::error::Error for Error {}

#[derive(Error, Debug)]
pub enum ErrorKind {
    #[error("Invalid param id {0:#x}")]
    InvalidParamId(u8),

    #[error("Hash param points out of bounds (index {0:#x})")]
    HashOutOfBounds(usize),

    #[error("String reference points out of bounds (start index {0:#x})")]
    StringRefOutOfBounds(usize),

    #[error("String data is not ascii (problem byte at {0:#x})")]
    StringNotAscii(usize),

    #[error(
        "Map reference points of out bounds (start index {start:#x}, num elements {num_elements})"
    )]
    MapRefOutOfBounds { start: usize, num_elements: usize },

    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error("{0}")]
    Custom(String),
}

impl serde::de::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        Self {
            cause: ErrorKind::Custom(msg.to_string()),
            position_stack: vec![],
        }
    }
}

impl<T> From<T> for Error
where
    ErrorKind: From<T>,
{
    fn from(value: T) -> Self {
        Self {
            cause: ErrorKind::from(value),
            position_stack: vec![],
        }
    }
}

macro_rules! tri {
    ($reader:expr, $parsing:ident, $e:expr) => {{
        let __position = $reader.stream_position().ok();
        let __result: Result<_, Error> = $e;
        match __result {
            Ok(__value) => __value,
            Err(mut __error) => {
                __error.position_stack.push((ParseId::$parsing, __position));
                return Err(__error);
            }
        }
    }};
}

macro_rules! tri_map {
    ($reader:expr, $parsing:ident, $e:expr) => {
        tri!($reader, $parsing, $e.map_err(Error::from))
    };
}

pub(crate) struct ReferenceData {
    file_offset: usize,
    raw: Vec<u8>,
    strings: HashMap<u32, String>,
    maps: HashMap<u32, Vec<(Hash40, u32)>>,
}

#[cfg(test)]
impl ReferenceData {
    pub fn mock(bytes: &[u8]) -> Self {
        Self {
            file_offset: 0,
            raw: bytes.to_vec(),
            strings: HashMap::new(),
            maps: HashMap::new(),
        }
    }

    pub fn new(bytes: Vec<u8>, file_offset: usize) -> Self {
        Self {
            file_offset,
            raw: bytes,
            strings: HashMap::new(),
            maps: HashMap::new(),
        }
    }

    pub fn empty() -> Self {
        Self {
            file_offset: 0,
            raw: vec![],
            strings: HashMap::new(),
            maps: HashMap::new(),
        }
    }
}

struct ParamFileReader<'a, R: Read + Seek> {
    reference: ReferenceData,
    hashes: &'a [Hash40],
    reader: &'a mut R,
    peeked_param_id: Option<ParamId>,
}

impl<'a, R: Read + Seek> Read for ParamFileReader<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

impl<'a, R: Read + Seek> Seek for ParamFileReader<'a, R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.reader.seek(pos)
    }
}

impl<'a, R: Read + Seek> ParamFileReader<'a, R> {
    fn read_param_id(&mut self) -> Result<ParamId, Error> {
        let param_id = tri_map!(self.reader, ParamId, self.reader.read_u8());
        Ok(tri_map!(
            self.reader,
            ParamId,
            ParamId::try_from(param_id).map_err(ErrorKind::InvalidParamId)
        ))
    }

    fn peek_param_id(&mut self) -> Result<ParamId, Error> {
        if let Some(peeked) = self.peeked_param_id {
            Ok(peeked)
        } else {
            let param = self.read_param_id()?;
            self.peeked_param_id = Some(param);
            Ok(param)
        }
    }

    fn next_param_id(&mut self) -> Result<ParamId, Error> {
        if let Some(peeked) = self.peeked_param_id.take() {
            Ok(peeked)
        } else {
            self.read_param_id()
        }
    }

    fn get_string(&mut self, offset: u32) -> Result<String, Error> {
        if let Some(cached) = self.reference.strings.get(&offset) {
            return Ok(cached.clone());
        }

        let offset = offset as usize;

        if offset >= self.reference.raw.len() {
            return Err(Error::from(ErrorKind::StringRefOutOfBounds(offset)));
        }

        let data = &self.reference.raw[offset..];
        let len =
            data.iter()
                .position(|byte| *byte == b'\0')
                .ok_or(ErrorKind::StringRefOutOfBounds(
                    self.reference.file_offset + offset,
                ))?;
        let string = &data[..len];
        if let Some(pos) = string.iter().position(|byte| !byte.is_ascii()) {
            return Err(Error::from(ErrorKind::StringNotAscii(
                self.reference.file_offset + offset + pos,
            )));
        }

        // SAFETY: We check that all chars are non-zero and ascii above
        let string = unsafe { std::str::from_utf8_unchecked(string).to_string() };
        self.reference.strings.insert(offset as u32, string.clone());
        Ok(string)
    }

    fn get_map(
        &mut self,
        offset: u32,
        len: usize,
        data_start: u64,
    ) -> Result<Vec<(Hash40, u64)>, Error> {
        if let Some(cached) = self.reference.maps.get(&offset) {
            return Ok(cached
                .iter()
                .map(|(hash, offset)| (*hash, data_start + *offset as u64))
                .collect());
        }

        let offset = offset as usize;

        if offset + len * 8 > self.reference.raw.len() {
            return Err(Error::from(ErrorKind::MapRefOutOfBounds {
                start: self.reference.file_offset + offset,
                num_elements: len,
            }));
        }

        let mut fields = Vec::with_capacity(len as usize);

        for index in 0..len {
            let local_hash_offset = offset + index * 8;
            let local_data_offset = local_hash_offset + 4;
            let hash_index =
                LittleEndian::read_u32(&self.reference.raw[local_hash_offset..local_data_offset])
                    as usize;
            let data_offset = LittleEndian::read_u32(
                &self.reference.raw[local_data_offset..local_data_offset + 4],
            );

            let Some(hash) = self.hashes.get(hash_index) else {
                return Err(Error::from(ErrorKind::HashOutOfBounds(hash_index)));
            };

            fields.push((*hash, data_offset));
        }

        self.reference.maps.insert(offset as u32, fields.clone());

        Ok(fields
            .into_iter()
            .map(|(hash, offset)| (hash, data_start + offset as u64))
            .collect())
    }
}

pub struct ValueDeserializer<'a, R: Read + Seek> {
    reader: ParamFileReader<'a, R>,
}

pub struct ListDeserializer<'a: 'b, 'b, R: Read + Seek> {
    offsets: Vec<u64>,
    current: usize,
    value_deserializer: &'b mut ValueDeserializer<'a, R>,
}

impl<'de, 'a: 'b, 'b, R: Read + Seek + 'de> SeqAccess<'de> for &mut ListDeserializer<'a, 'b, R> {
    type Error = Error;

    fn size_hint(&self) -> Option<usize> {
        Some(self.offsets.len())
    }

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        match self.offsets.get(self.current) {
            Some(offset) => {
                let _ = tri_map!(
                    self.value_deserializer.reader,
                    ParamId,
                    self.value_deserializer
                        .reader
                        .seek(SeekFrom::Start(*offset))
                );

                self.current += 1;
                let value = tri!(
                    self.value_deserializer.reader,
                    ParamId,
                    seed.deserialize(&mut *self.value_deserializer)
                );

                Ok(Some(value))
            }
            None => Ok(None),
        }
    }
}

pub struct MapDeserializer<'a: 'b, 'b, R: Read + Seek> {
    keys: Vec<(Hash40, u64)>,
    current: usize,
    current_key: usize,
    fields: Option<&'static [&'static str]>,
    value_deserializer: &'b mut ValueDeserializer<'a, R>,
}

impl<'de, 'a: 'b, 'b, R: Read + Seek + 'de> MapAccess<'de> for &mut MapDeserializer<'a, 'b, R> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        if self.current >= self.keys.len() {
            return Ok(None);
        }

        let key = self.keys[self.current].0;
        let map_key = if let Some(field) = self
            .fields
            .and_then(|fields| fields.iter().find(|field| hash40::hash40(*field) == key))
        {
            MapKeyDeserializer::Member(*field)
        } else {
            MapKeyDeserializer::Hash(key)
        };

        let key = tri!(
            self.value_deserializer.reader,
            Map,
            seed.deserialize(map_key)
        );

        self.current_key = self.current;
        self.current += 1;

        Ok(Some(key))
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        if self.current_key == self.current {
            return Err(Error::from(ErrorKind::Custom(
                "logical error requesting value before key".to_string(),
            )));
        }

        let offset = self.keys[self.current_key].1;
        tri_map!(
            self.value_deserializer.reader,
            Map,
            self.value_deserializer.reader.seek(SeekFrom::Start(offset))
        );

        let result = tri!(
            self.value_deserializer.reader,
            Map,
            seed.deserialize(&mut *self.value_deserializer)
        );

        self.current_key = self.current;

        Ok(result)
    }
}

impl<'a, R: Read + Seek> ValueDeserializer<'a, R> {
    pub(crate) fn new(
        reference_data: ReferenceData,
        hashes: &'a [Hash40],
        reader: &'a mut R,
    ) -> Self {
        Self {
            reader: ParamFileReader {
                reference: reference_data,
                hashes,
                reader,
                peeked_param_id: None,
            },
        }
    }

    fn deserialize_map<'de, V: Visitor<'de>>(
        &mut self,
        fields: Option<&'static [&'static str]>,
        visitor: V,
    ) -> Result<V::Value, Error>
    where
        R: 'de,
    {
        // Subtract 1 from the current position to get the base offset all of the elemenets
        // are relative to
        let base_position = tri_map!(self.reader, Map, self.reader.stream_position())
            .checked_sub(1)
            .unwrap();

        let num_elements =
            tri_map!(self.reader, Map, self.reader.read_u32::<LittleEndian>()) as usize;

        let ref_position = tri_map!(self.reader, Map, self.reader.read_u32::<LittleEndian>());

        let keys = tri!(
            self.reader,
            Map,
            self.reader
                .get_map(ref_position, num_elements, base_position)
        );

        let mut map_deserializer = MapDeserializer {
            keys,
            current: 0,
            current_key: 0,
            fields,
            value_deserializer: self,
        };

        let result = visitor.visit_map(&mut map_deserializer);

        // If the map deserializer finishes prematurely, we need to parse the last value
        // so that we can advance to the correct cursor position
        if map_deserializer.current < num_elements {
            let offset = map_deserializer.keys.last().unwrap().1;
            tri_map!(self.reader, Map, self.reader.seek(SeekFrom::Start(offset)));
            tri!(self.reader, Map, Value::deserialize(&mut *self));
        }

        Ok(tri!(self.reader, Map, result))
    }
}

impl<'de, 'a, R: Read + Seek + 'de> Deserializer<'de> for &mut ValueDeserializer<'a, R> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        use ParamId as P;

        match self.reader.next_param_id()? {
            P::Bool => {
                let value = tri_map!(self.reader, Bool, self.reader.read_u8());
                Ok(tri!(self.reader, Bool, visitor.visit_bool(value != 0)))
            }
            P::I8 => {
                let value = tri_map!(self.reader, I8, self.reader.read_i8());
                Ok(tri!(self.reader, I8, visitor.visit_i8(value)))
            }
            P::U8 => {
                let value = tri_map!(self.reader, U8, self.reader.read_u8());
                Ok(tri!(self.reader, U8, visitor.visit_u8(value)))
            }
            P::I16 => {
                let value = tri_map!(self.reader, I16, self.reader.read_i16::<LittleEndian>());
                Ok(tri!(self.reader, I16, visitor.visit_i16(value)))
            }
            P::U16 => {
                let value = tri_map!(self.reader, U16, self.reader.read_u16::<LittleEndian>());
                Ok(tri!(self.reader, U16, visitor.visit_u16(value)))
            }
            P::I32 => {
                let value = tri_map!(self.reader, I32, self.reader.read_i32::<LittleEndian>());
                Ok(tri!(self.reader, I32, visitor.visit_i32(value)))
            }
            P::U32 => {
                let value = tri_map!(self.reader, U32, self.reader.read_u32::<LittleEndian>());
                Ok(tri!(self.reader, U32, visitor.visit_u32(value)))
            }
            P::F32 => {
                let value = tri_map!(self.reader, F32, self.reader.read_f32::<LittleEndian>());
                Ok(tri!(self.reader, F32, visitor.visit_f32(value)))
            }
            P::Hash => {
                let index =
                    tri_map!(self.reader, Hash, self.reader.read_u32::<LittleEndian>()) as usize;

                let position = self.reader.stream_position().ok();
                let Some(hash) = self.reader.hashes.get(index).copied() else {
                    return Err(Error {
                        cause: ErrorKind::HashOutOfBounds(index),
                        position_stack: vec![(ParseId::Hash, position)],
                    });
                };

                Ok(tri!(self.reader, Hash, visitor.visit_u64(hash.0)))
            }
            P::String => {
                let ref_offset =
                    tri_map!(self.reader, String, self.reader.read_u32::<LittleEndian>());

                let string = tri!(self.reader, String, self.reader.get_string(ref_offset));

                Ok(tri!(self.reader, String, visitor.visit_string(string)))
            }
            P::List => {
                // Subtract 1 from the current position to get the base offset all of the elemenets
                // are relative to
                let base_position = tri_map!(self.reader, List, self.reader.stream_position())
                    .checked_sub(1)
                    .unwrap();

                let num_elements =
                    tri_map!(self.reader, List, self.reader.read_u32::<LittleEndian>());

                let mut offsets = Vec::with_capacity(num_elements as usize);

                for _ in 0..num_elements {
                    let el_offset =
                        tri_map!(self.reader, List, self.reader.read_u32::<LittleEndian>());
                    offsets.push(base_position + el_offset as u64);
                }

                let mut list_deserializer = ListDeserializer {
                    offsets,
                    current: 0,
                    value_deserializer: self,
                };

                let value = visitor.visit_seq(&mut list_deserializer);

                // If the list deserializer finishes prematurely, we need to parse the last value
                // so that we can advance to the correct cursor position
                if list_deserializer.current < list_deserializer.offsets.len() {
                    let offset = *list_deserializer.offsets.last().unwrap();
                    tri_map!(self.reader, List, self.reader.seek(SeekFrom::Start(offset)));
                    tri!(self.reader, List, Value::deserialize(&mut *self));
                }

                Ok(tri!(self.reader, List, value))
            }
            P::Map => self.deserialize_map(None, visitor),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        if self.reader.peek_param_id()? == ParamId::Hash {
            let index =
                tri_map!(self.reader, Hash, self.reader.read_u32::<LittleEndian>()) as usize;

            let position = self.reader.stream_position().ok();
            let Some(hash) = self.reader.hashes.get(index).copied() else {
                return Err(Error {
                    cause: ErrorKind::HashOutOfBounds(index),
                    position_stack: vec![(ParseId::Hash, position)],
                });
            };

            Ok(tri!(
                self.reader,
                Hash,
                visitor.visit_string(format!("{hash}"))
            ))
        } else {
            self.deserialize_any(visitor)
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if self.reader.peek_param_id()? == ParamId::Map {
            let _ = self.reader.next_param_id();
            self.deserialize_map(Some(fields), visitor)
        } else {
            self.deserialize_any(visitor)
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map enum identifier ignored_any
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Value::I8(v))
    }

    fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Value::U8(v))
    }

    fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Value::I16(v))
    }

    fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Value::U16(v))
    }

    fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Value::I32(v))
    }

    fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Value::U32(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Value::I32(v.try_into().map_err(|e| E::custom(e))?))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Value::Hash(Hash40(v)))
    }

    fn visit_f32<E>(self, v: f32) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Value::F32(v))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Value::F32(v as f32))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if v.starts_with("0x") && v.len() == "0x123456789A".len() {
            match u64::from_str_radix(v.strip_prefix("0x").unwrap(), 16) {
                Ok(v) => Ok(Value::Hash(Hash40(v))),
                Err(_) => Ok(Value::String(v.to_string())),
            }
        } else {
            Ok(Value::String(v.to_string()))
        }
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(v.as_str())
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut list = Vec::with_capacity(seq.size_hint().unwrap_or_default());

        while let Some(next) = seq.next_element::<Value>()? {
            list.push(next);
        }

        Ok(Value::List(list))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut object = IndexMap::with_capacity(map.size_hint().unwrap_or_default());

        while let Some((k, v)) = map.next_entry::<Hash40, Value>()? {
            object.insert(k, v);
        }

        Ok(Value::Map(object))
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Value::Bool(v))
    }

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a prc value")
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ValueVisitor)
    }
}

enum MapKeyDeserializer {
    Hash(Hash40),
    Member(&'static str),
}

impl<'de> Deserializer<'de> for MapKeyDeserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Hash(hash) => visitor.visit_u64(hash.0),
            Self::Member(member) => visitor.visit_str(member),
        }
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Hash(hash) => visitor.visit_u64(hash.0),
            Self::Member(member) => visitor.visit_u64(hash40::hash40(member).0),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Hash(hash) => visitor.visit_string(format!("{hash}")),
            Self::Member(member) => visitor.visit_str(member),
        }
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            Self::Hash(hash) => visitor.visit_string(format!("{hash}")),
            Self::Member(member) => visitor.visit_string(member.to_string()),
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u128 f32 f64 char
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}
