use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hasher},
    io::{Cursor, Write},
};

use byteorder::{LittleEndian, WriteBytesExt};
use hash40::Hash40;
use indexmap::{IndexMap, IndexSet};
use serde::{
    ser::{
        Impossible, SerializeMap, SerializeSeq, SerializeStruct, SerializeTuple,
        SerializeTupleStruct,
    },
    Serialize, Serializer,
};

use crate::{ParamId, Value};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Unsupported key type: '{0}'")]
    UnsupportedKeyType(&'static str),

    #[error("Unsupported value type: '{0}'")]
    UnsupportedValueType(&'static str),

    #[error(transparent)]
    IO(#[from] std::io::Error),

    #[error("{0}")]
    Custom(String),
}

impl serde::ser::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        Self::Custom(msg.to_string())
    }
}

pub struct IntoValueSerializer;

pub struct ListSerializer(Vec<Value>);

pub struct MapSerializer {
    map: IndexMap<Hash40, Value>,
    current_key: Option<Hash40>,
}

impl SerializeSeq for ListSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.0.push(value.serialize(IntoValueSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::List(self.0))
    }
}

impl SerializeTuple for ListSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.0.push(value.serialize(IntoValueSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::List(self.0))
    }
}

impl SerializeTupleStruct for ListSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        self.0.push(value.serialize(IntoValueSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::List(self.0))
    }
}

impl SerializeMap for MapSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        let key = key.serialize(HashSerializer)?;
        self.current_key = Some(key);
        Ok(())
    }

    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        if !self.current_key.is_some() {
            return Err(Error::Custom(
                "attempting to serialize value with no key".to_string(),
            ));
        }

        let value = value.serialize(IntoValueSerializer)?;
        let key = self.current_key.take().unwrap();

        self.map.insert(key, value);

        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Map(self.map))
    }
}

impl SerializeStruct for MapSerializer {
    type Ok = Value;
    type Error = Error;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: Serialize,
    {
        let key = hash40::hash40(key);
        let value = value.serialize(IntoValueSerializer)?;

        self.map.insert(key, value);

        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Map(self.map))
    }
}

macro_rules! e {
    ($e:literal) => {
        Err(Error::UnsupportedValueType($e))
    };
}

impl Serializer for IntoValueSerializer {
    type Ok = Value;
    type Error = Error;

    type SerializeSeq = ListSerializer;

    type SerializeTuple = ListSerializer;

    type SerializeTupleStruct = ListSerializer;

    type SerializeTupleVariant = Impossible<Value, Error>;

    type SerializeMap = MapSerializer;

    type SerializeStruct = MapSerializer;

    type SerializeStructVariant = Impossible<Value, Error>;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        Ok(Value::I8(v))
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        Ok(Value::U8(v))
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        Ok(Value::I16(v))
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        Ok(Value::U16(v))
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        Ok(Value::I32(v))
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        Ok(Value::U32(v))
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::I32(
            v.try_into().map_err(<Error as serde::ser::Error>::custom)?,
        ))
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Hash(Hash40(v)))
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        Ok(Value::F32(v))
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::F32(v as f32))
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        Ok(Value::String(v.to_string()))
    }

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Bool(v))
    }

    fn serialize_char(self, _v: char) -> Result<Self::Ok, Self::Error> {
        e!("char")
    }

    fn serialize_bytes(self, _v: &[u8]) -> Result<Self::Ok, Self::Error> {
        e!("&[u8]")
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        e!("none")
    }

    fn serialize_some<T: ?Sized>(self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        e!("some")
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        e!("unit")
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(Value::String(name.to_string()))
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Ok(Value::String(variant.to_string()))
    }

    fn serialize_newtype_struct<T: ?Sized>(
        self,
        _name: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        e!("newtype struct")
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        e!("newtype variant")
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(ListSerializer(Vec::with_capacity(len.unwrap_or_default())))
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(ListSerializer(Vec::with_capacity(len)))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(ListSerializer(Vec::with_capacity(len)))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        e!("tuple variant")
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(MapSerializer {
            map: IndexMap::with_capacity(len.unwrap_or_default()),
            current_key: None,
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(MapSerializer {
            map: IndexMap::with_capacity(len),
            current_key: None,
        })
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        e!("struct variant")
    }
}

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

struct HashSerializer;

macro_rules! key_err {
    ($e:literal) => {
        Err(Error::UnsupportedKeyType($e))
    };
}

impl Serializer for HashSerializer {
    type Ok = Hash40;
    type Error = Error;

    type SerializeSeq = Impossible<Hash40, Error>;
    type SerializeMap = Impossible<Hash40, Error>;
    type SerializeTuple = Impossible<Hash40, Error>;
    type SerializeTupleStruct = Impossible<Hash40, Error>;
    type SerializeStruct = Impossible<Hash40, Error>;
    type SerializeTupleVariant = Impossible<Hash40, Error>;
    type SerializeStructVariant = Impossible<Hash40, Error>;

    fn serialize_bool(self, _v: bool) -> Result<Self::Ok, Self::Error> {
        key_err!("bool")
    }

    fn serialize_i8(self, _v: i8) -> Result<Self::Ok, Self::Error> {
        key_err!("i8")
    }

    fn serialize_i16(self, _v: i16) -> Result<Self::Ok, Self::Error> {
        key_err!("i16")
    }

    fn serialize_i32(self, _v: i32) -> Result<Self::Ok, Self::Error> {
        key_err!("i32")
    }

    fn serialize_i64(self, _v: i64) -> Result<Self::Ok, Self::Error> {
        key_err!("i64")
    }

    fn serialize_u8(self, _v: u8) -> Result<Self::Ok, Self::Error> {
        key_err!("u8")
    }

    fn serialize_u16(self, _v: u16) -> Result<Self::Ok, Self::Error> {
        key_err!("u16")
    }

    fn serialize_u32(self, _v: u32) -> Result<Self::Ok, Self::Error> {
        key_err!("u32")
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        Ok(Hash40(v))
    }

    fn serialize_f32(self, _v: f32) -> Result<Self::Ok, Self::Error> {
        key_err!("f32")
    }

    fn serialize_f64(self, _v: f64) -> Result<Self::Ok, Self::Error> {
        key_err!("f64")
    }

    fn serialize_char(self, _v: char) -> Result<Self::Ok, Self::Error> {
        key_err!("char")
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        Ok(hash40::hash40(v))
    }

    fn serialize_bytes(self, _v: &[u8]) -> Result<Self::Ok, Self::Error> {
        key_err!("&[u8]")
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        key_err!("none")
    }

    fn serialize_some<T: ?Sized>(self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        key_err!("some")
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        key_err!("unit")
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(hash40::hash40(name))
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Ok(hash40::hash40(variant))
    }

    fn serialize_newtype_struct<T: ?Sized>(
        self,
        _name: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        key_err!("newtype struct")
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: Serialize,
    {
        key_err!("newtype variant")
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        key_err!("sequence")
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        key_err!("tuple")
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        key_err!("tuple struct")
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        key_err!("tuple variant")
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        key_err!("map")
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        key_err!("struct")
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        key_err!("struct variant")
    }
}

const fn prim<T: Sized>() -> usize {
    1 + std::mem::size_of::<T>()
}

fn calculate_binary_size_of_value(value: &Value) -> usize {
    match value {
        Value::Bool(_) | Value::U8(_) | Value::I8(_) => prim::<u8>(),
        Value::I16(_) | Value::U16(_) => prim::<u16>(),
        Value::I32(_) | Value::U32(_) | Value::F32(_) | Value::String(_) | Value::Hash(_) => {
            prim::<u32>()
        }
        Value::List(values) => {
            prim::<u32>()
                + values.len() * std::mem::size_of::<u32>()
                + values
                    .iter()
                    .map(calculate_binary_size_of_value)
                    .sum::<usize>()
        }
        Value::Map(map) => {
            prim::<u32>()
                + std::mem::size_of::<u32>()
                + map
                    .values()
                    .map(calculate_binary_size_of_value)
                    .sum::<usize>()
        }
    }
}

fn visit_hashes(lookup: &mut IndexSet<Hash40>, value: &Value) {
    match value {
        Value::Hash(hash) => {
            lookup.insert(*hash);
        }
        Value::List(values) => {
            values.iter().for_each(|value| visit_hashes(lookup, value));
        }
        Value::Map(values) => {
            for (k, v) in values.iter() {
                lookup.insert(*k);
                visit_hashes(lookup, v);
            }
        }
        _ => {}
    }
}

fn visit_strings(data: &mut Vec<u8>, lookup: &mut HashMap<String, u32>, value: &Value) {
    match value {
        Value::String(string) if !lookup.contains_key(string) => {
            let offset = data.len() as u32;
            data.extend_from_slice(string.as_bytes());
            data.push(b'\0');
            lookup.insert(string.clone(), offset);
        }
        Value::List(values) => values
            .iter()
            .for_each(|value| visit_strings(data, lookup, value)),
        Value::Map(map) => map
            .values()
            .for_each(|value| visit_strings(data, lookup, value)),
        _ => {}
    }
}

fn get_struct_key(map: &IndexMap<Hash40, Value>) -> u64 {
    use std::hash::Hash;
    let mut hasher = DefaultHasher::default();
    for (key, value) in map.iter() {
        key.hash(&mut hasher);
        calculate_binary_size_of_value(value).hash(&mut hasher);
    }

    hasher.finish()
}

fn visit_structs(
    hashes: &IndexSet<Hash40>,
    data: &mut Vec<u8>,
    lookup: &mut HashMap<u64, u32>,
    value: &Value,
) {
    match value {
        Value::List(list) => {
            for value in list.iter() {
                visit_structs(hashes, data, lookup, value);
            }
        }
        Value::Map(map) => {
            let key = get_struct_key(map);

            if lookup.contains_key(&key) {
                return;
            }

            let ref_offset = data.len() as u32;
            let mut wip_offset = prim::<u32>() + std::mem::size_of::<u32>();
            for (key, value) in map.iter() {
                let key_index = hashes
                    .get_index_of(key)
                    .expect("should have cached the map key");
                let value_offset = wip_offset;
                wip_offset += calculate_binary_size_of_value(value);
                data.write_u32::<LittleEndian>(key_index as u32)
                    .expect("writing to vec");
                data.write_u32::<LittleEndian>(value_offset as u32)
                    .expect("writing to vec");
            }

            lookup.insert(key, ref_offset);

            for value in map.values() {
                visit_structs(hashes, data, lookup, value);
            }
        }
        _ => {}
    }
}

fn write_value<W: Write>(
    writer: &mut W,
    hashes: &IndexSet<Hash40>,
    strings: &HashMap<String, u32>,
    structs: &HashMap<u64, u32>,
    value: &Value,
) -> Result<(), Error> {
    match value {
        Value::Bool(v) => {
            writer.write_u8(ParamId::Bool as u8)?;
            writer.write_u8(*v as u8)?;
        }
        Value::I8(v) => {
            writer.write_u8(ParamId::I8 as u8)?;
            writer.write_i8(*v)?;
        }
        Value::U8(v) => {
            writer.write_u8(ParamId::U8 as u8)?;
            writer.write_u8(*v)?;
        }
        Value::I16(v) => {
            writer.write_u8(ParamId::I16 as u8)?;
            writer.write_i16::<LittleEndian>(*v)?;
        }
        Value::U16(v) => {
            writer.write_u8(ParamId::U16 as u8)?;
            writer.write_u16::<LittleEndian>(*v)?;
        }
        Value::I32(v) => {
            writer.write_u8(ParamId::I32 as u8)?;
            writer.write_i32::<LittleEndian>(*v)?;
        }
        Value::U32(v) => {
            writer.write_u8(ParamId::U32 as u8)?;
            writer.write_u32::<LittleEndian>(*v)?;
        }
        Value::F32(v) => {
            writer.write_u8(ParamId::F32 as u8)?;
            writer.write_f32::<LittleEndian>(*v)?;
        }
        Value::Hash(v) => {
            writer.write_u8(ParamId::Hash as u8)?;
            writer.write_u32::<LittleEndian>(
                hashes.get_index_of(v).expect("should have cached hash") as u32,
            )?;
        }
        Value::String(v) => {
            writer.write_u8(ParamId::String as u8)?;
            writer
                .write_u32::<LittleEndian>(*strings.get(v).expect("should have cached string"))?;
        }
        Value::List(v) => {
            writer.write_u8(ParamId::List as u8)?;
            writer.write_u32::<LittleEndian>(v.len() as u32)?;
            let mut wip_offset = (prim::<u32>() + v.len() * std::mem::size_of::<u32>()) as u32;
            for value in v.iter() {
                writer.write_u32::<LittleEndian>(wip_offset)?;
                wip_offset += calculate_binary_size_of_value(value) as u32;
            }
            for value in v.iter() {
                write_value(writer, hashes, strings, structs, value)?;
            }
        }
        Value::Map(map) => {
            writer.write_u8(ParamId::Map as u8)?;
            writer.write_u32::<LittleEndian>(map.len() as u32)?;
            writer.write_u32::<LittleEndian>(
                *structs
                    .get(&get_struct_key(map))
                    .expect("should have cached struct"),
            )?;
            for value in map.values() {
                write_value(writer, hashes, strings, structs, value)?;
            }
        }
    }

    Ok(())
}

pub fn write<W: Write, T: Serialize>(mut writer: W, value: &T) -> Result<(), Error> {
    let value = value.serialize(IntoValueSerializer)?;

    let mut hash_lookup = IndexSet::with_capacity(64);
    let mut reference_data = Vec::with_capacity(128);
    let mut string_lookup = HashMap::new();
    let mut struct_lookup = HashMap::new();
    visit_hashes(&mut hash_lookup, &value);
    visit_strings(&mut reference_data, &mut string_lookup, &value);
    visit_structs(
        &hash_lookup,
        &mut reference_data,
        &mut struct_lookup,
        &value,
    );
    writer.write_all(b"paracobn")?;

    writer.write_u32::<LittleEndian>(8 * hash_lookup.len() as u32)?;
    writer.write_u32::<LittleEndian>(reference_data.len() as u32)?;

    for hash in hash_lookup.iter() {
        writer.write_u64::<LittleEndian>(hash.0)?;
    }

    writer.write_all(&reference_data)?;

    write_value(
        &mut writer,
        &hash_lookup,
        &string_lookup,
        &struct_lookup,
        &value,
    )?;

    Ok(())
}

pub fn to_vec<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    let mut writer = Cursor::new(Vec::with_capacity(256));
    write(&mut writer, value)?;

    Ok(writer.into_inner())
}
