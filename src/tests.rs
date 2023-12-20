use crate::de::{ReferenceData, ValueDeserializer};
use hash40::{hash40, Hash40};
use serde::Deserialize;
use serial_test::serial;

macro_rules! deserializer {
    ($slice:expr) => {{
        &mut ValueDeserializer::new(
            ReferenceData::empty(),
            &[],
            &mut std::io::Cursor::new($slice),
        )
    }};
    ($hashes:expr, $slice:expr) => {{
        &mut ValueDeserializer::new(
            ReferenceData::empty(),
            &$hashes,
            &mut std::io::Cursor::new($slice),
        )
    }};
    ($reference:expr, $hashes:expr, $slice:expr) => {{
        &mut ValueDeserializer::new($reference, &$hashes, &mut std::io::Cursor::new($slice))
    }};
}

macro_rules! decl_value_tests {
    ($([$test_name:ident, $type:path] => {$($name:ident: [$($data:literal),*] = $value:expr,)*});*) => {
        $(
            #[test]
            #[serial]
            fn $test_name() {
                $(
                    const $name: &[u8] = &[$($data),*];
                    assert_eq!(<$type>::deserialize(deserializer!($name)).unwrap(), $value);
                )*
            }
        )*
    }
}

decl_value_tests! {
    [deserialize_bool, bool] => {
        FALSE: [0x01, 0x00] = false,
        TRUE: [0x01, 0x01] = true,
    };
    [deserialize_i8, i8] => {
        POSITIVE: [0x02, 33] = 33,
        NEGATIVE: [0x02, 128] = -128,
    };
    [deserialize_u8, u8] => {
        DATA: [0x03, 192] = 192,
    };
    [deserialize_i16, i16] => {
        POSITIVE: [0x04, 0xF6, 0x3F] = 16374,
        NEGATIVE: [0x04, 0xE3, 0xF2] = -3357,
    };
    [deserialize_u16, u16] => {
        DATA: [0x05, 0xA7, 0x9B] = 39847,
    };
    [deserialize_i32, i32] => {
        POSITIVE: [0x06, 0x6C, 0xFB, 0xC1, 0x19] = 432143212,
        NEGATIVE: [0x06, 0x4F, 0xEE, 0xF5, 0xFD] = -34214321,
    };
    [deserialize_u32, u32] => {
        DATA: [0x07, 0xDB, 0x29, 0x29, 0xC5] = 3307809243,
    };
    [deserialize_f32, f32] => {
        DATA: [0x08, 0x00, 0x74, 0x56, 0x45] = 3431.25,
    }
}

mod hash {
    use super::*;
    const FIRST: &[u8] = &[0x09, 0x00, 0x00, 0x00, 0x00];
    const SECOND: &[u8] = &[0x09, 0x01, 0x00, 0x00, 0x00];

    const FOO: Hash40 = hash40("foo");
    const BAR: Hash40 = hash40("bar");

    const HASHES: [Hash40; 2] = [FOO, BAR];

    #[test]
    #[serial]
    fn deserialize_hash() {
        Hash40::label_map().lock().unwrap().clear();
        assert_eq!(
            Hash40::deserialize(deserializer!(HASHES, FIRST)).unwrap(),
            FOO
        );
        assert_eq!(
            Hash40::deserialize(deserializer!(HASHES, SECOND)).unwrap(),
            BAR
        );
    }

    #[test]
    #[serial]
    fn deserialize_hash_as_u64() {
        Hash40::label_map().lock().unwrap().clear();
        assert_eq!(
            u64::deserialize(deserializer!(HASHES, FIRST)).unwrap(),
            FOO.0
        );
        assert_eq!(
            u64::deserialize(deserializer!(HASHES, SECOND)).unwrap(),
            BAR.0
        );
    }

    #[test]
    #[serial]
    fn deserialize_hash_as_str() {
        Hash40::label_map().lock().unwrap().clear();

        assert_eq!(
            String::deserialize(deserializer!(HASHES, FIRST)).unwrap(),
            "0x038c736521"
        );
        assert_eq!(
            String::deserialize(deserializer!(HASHES, SECOND)).unwrap(),
            "0x0376ff8caa"
        );
    }

    #[test]
    #[serial]
    fn deserialize_hash_with_labels() {
        Hash40::label_map().lock().unwrap().clear();
        Hash40::label_map()
            .lock()
            .unwrap()
            .add_labels(vec!["foo".to_string(), "bar".to_string()]);

        assert_eq!(
            String::deserialize(deserializer!(HASHES, FIRST)).unwrap(),
            "foo"
        );
        assert_eq!(
            String::deserialize(deserializer!(HASHES, SECOND)).unwrap(),
            "bar"
        );
    }
}

#[test]
#[serial]
fn deserialize_string() {
    const FIRST: &[u8] = &[0x0A, 0x00, 0x00, 0x00, 0x00]; // Should pass: foo!
    const SECOND: &[u8] = &[0x0A, 0x05, 0x00, 0x00, 0x00]; // Should pass: bar...
    const THIRD: &[u8] = &[0x0A, 0x0C, 0x00, 0x00, 0x00]; // Should fail: no null term
    const FOURTH: &[u8] = &[0x0A, 0x00, 0x00, 0x00, 0x00]; // Should fail: invalid ascii

    const REFERENCE_BYTES: &[u8] = b"foo!\0bar...\0baz???";
    const REF_BYTES_4: &[u8] = &[b'f', b'a', b'z', 182, b'\0'];

    assert_eq!(
        String::deserialize(deserializer!(
            ReferenceData::mock(REFERENCE_BYTES),
            [],
            FIRST
        ))
        .unwrap(),
        "foo!"
    );
    assert_eq!(
        String::deserialize(deserializer!(
            ReferenceData::mock(REFERENCE_BYTES),
            [],
            SECOND
        ))
        .unwrap(),
        "bar..."
    );

    assert!(String::deserialize(deserializer!(
        ReferenceData::mock(REFERENCE_BYTES),
        [],
        THIRD
    ))
    .is_err());

    assert!(
        String::deserialize(deserializer!(ReferenceData::mock(REF_BYTES_4), [], FOURTH)).is_err()
    );
}

#[test]
#[serial]
fn deserialize_list() {
    const DATA: &[u8] = &[
        0x0B, // Param ID (list)
        0x04, 0x00, 0x00, 0x00, // Size of the list
        0x16, 0x00, 0x00, 0x00, // Offset of the first
        0x1B, 0x00, 0x00, 0x00, // Offset of the second
        0x20, 0x00, 0x00, 0x00, // Offset of the third
        0x25, 0x00, 0x00, 0x00, 0x00, // Offset of the fourth
        0x06, 0x06, 0x00, 0x00, 0x00, // First value (6i32)
        0x06, 0x01, 0x00, 0x00, 0x00, // Second value (1i32)
        0x06, 0x0C, 0x00, 0x00, 0x00, // Third value (12i32)
        0x01, 0x01, // Fourth value (should be skipped)
        0x0B, // Param ID (list)
        0x03, 0x00, 0x00, 0x00, // Size of the list
        0x11, 0x00, 0x00, 0x00, // Offset of the first
        0x16, 0x00, 0x00, 0x00, // Offset of the second
        0x1B, 0x00, 0x00, 0x00, // Offset of the third
        0x06, 0x13, 0x00, 0x00, 0x00, // First value (19i32)
        0x06, 0x80, 0x00, 0x00, 0x00, // Second value (128i32)
        0x06, 0xFF, 0xFF, 0xFF, 0xFF, // Third value (-1i32)
    ];

    let mut cursor = std::io::Cursor::new(DATA);

    let mut deserializer = ValueDeserializer::new(ReferenceData::empty(), &[], &mut cursor);

    // Test to make sure we properly deserialized the first values
    assert_eq!(
        <[i32; 3]>::deserialize(&mut deserializer).unwrap(),
        [6i32, 1i32, 12i32]
    );

    // Test to make sure we skipped the bool and deserialized the second array
    assert_eq!(
        <[i32; 3]>::deserialize(&mut deserializer).unwrap(),
        [19i32, 128i32, -1i32]
    );
}
