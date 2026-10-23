use mythos_strucpp::ffi::{LocatedArea, LocatedSize, LocatedVarDescriptor, LocatedVarRaw};
use std::ffi::c_void;

#[test]
fn test_located_var_raw_size_and_alignment() {
    assert_eq!(std::mem::size_of::<LocatedVarRaw>(), 16);
    assert_eq!(std::mem::align_of::<LocatedVarRaw>(), 8);
}

#[test]
fn test_located_var_descriptor_parsing() {
    let mut dummy_val: u16 = 1234;
    let raw = LocatedVarRaw {
        area: 0, // Input
        size: 2, // Word
        byte_index: 10,
        bit_index: 0,
        _reserved: [0; 3],
        pointer: &mut dummy_val as *mut u16 as *mut c_void,
    };

    let desc = LocatedVarDescriptor::from_raw(&raw).expect("valid descriptor");
    assert_eq!(desc.area, LocatedArea::Input);
    assert_eq!(desc.size, LocatedSize::Word);
    assert_eq!(desc.byte_index, 10);
    assert_eq!(desc.bit_index, 0);
    assert_eq!(desc.iec_address(), "%IW10");

    unsafe {
        assert_eq!(desc.read_u64(), 1234);
        desc.write_u64(5678);
        assert_eq!(desc.read_u64(), 5678);
        assert_eq!(dummy_val, 5678);
    }
}

#[test]
fn test_located_bit_descriptor() {
    let mut bool_val: u8 = 0;
    let raw = LocatedVarRaw {
        area: 1, // Output
        size: 0, // Bit
        byte_index: 2,
        bit_index: 5,
        _reserved: [0; 3],
        pointer: &mut bool_val as *mut u8 as *mut c_void,
    };

    let desc = LocatedVarDescriptor::from_raw(&raw).expect("valid descriptor");
    assert_eq!(desc.area, LocatedArea::Output);
    assert_eq!(desc.size, LocatedSize::Bit);
    assert_eq!(desc.iec_address(), "%QX2.5");

    unsafe {
        assert!(!desc.read_bool());
        desc.write_bool(true);
        assert!(desc.read_bool());
        assert_eq!(bool_val, 1);
    }
}

#[test]
fn test_invalid_descriptor_error() {
    let raw = LocatedVarRaw {
        area: 99, // Invalid
        size: 0,
        byte_index: 0,
        bit_index: 0,
        _reserved: [0; 3],
        pointer: std::ptr::null_mut(),
    };

    let res = LocatedVarDescriptor::from_raw(&raw);
    assert!(res.is_err());
}
