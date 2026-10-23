//! Cache-line aligned Plain Old Data (POD) tag slot layout and value primitives.

use bytemuck::{Pod, Zeroable};

/// Raw 64-bit value union representing any fundamental industrial process data type.
#[derive(Copy, Clone)]
#[repr(C)]
pub union TagValueRaw {
    pub boolean: bool,
    pub u8_val: u8,
    pub i8_val: i8,
    pub u16_val: u16,
    pub i16_val: i16,
    pub u32_val: u32,
    pub i32_val: i32,
    pub u64_val: u64,
    pub i64_val: i64,
    pub f32_val: f32,
    pub f64_val: f64,
    pub raw_bytes: [u8; 8],
}

impl std::fmt::Debug for TagValueRaw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Formatted safely by displaying raw 64-bit integer / byte representation
        unsafe {
            f.debug_struct("TagValueRaw")
                .field("u64", &self.u64_val)
                .field("f64", &self.f64_val)
                .field("raw_bytes", &self.raw_bytes)
                .finish()
        }
    }
}

impl PartialEq for TagValueRaw {
    fn eq(&self, other: &Self) -> bool {
        unsafe { self.raw_bytes == other.raw_bytes }
    }
}

impl Eq for TagValueRaw {}

unsafe impl Zeroable for TagValueRaw {}
unsafe impl Pod for TagValueRaw {}

impl TagValueRaw {
    #[inline]
    pub const fn from_bool(val: bool) -> Self {
        let mut bytes = [0u8; 8];
        bytes[0] = val as u8;
        Self { raw_bytes: bytes }
    }

    #[inline]
    pub const fn from_u8(val: u8) -> Self {
        let mut bytes = [0u8; 8];
        bytes[0] = val;
        Self { raw_bytes: bytes }
    }

    #[inline]
    pub const fn from_i16(val: i16) -> Self {
        Self {
            raw_bytes: (val as i64).to_le_bytes(),
        }
    }

    #[inline]
    pub const fn from_u16(val: u16) -> Self {
        Self {
            raw_bytes: (val as u64).to_le_bytes(),
        }
    }

    #[inline]
    pub const fn from_i32(val: i32) -> Self {
        Self {
            raw_bytes: (val as i64).to_le_bytes(),
        }
    }

    #[inline]
    pub const fn from_u32(val: u32) -> Self {
        Self {
            raw_bytes: (val as u64).to_le_bytes(),
        }
    }

    #[inline]
    pub const fn from_i64(val: i64) -> Self {
        Self {
            raw_bytes: val.to_le_bytes(),
        }
    }

    #[inline]
    pub const fn from_u64(val: u64) -> Self {
        Self {
            raw_bytes: val.to_le_bytes(),
        }
    }

    #[inline]
    pub fn from_f32(val: f32) -> Self {
        Self {
            raw_bytes: (val as f64).to_bits().to_le_bytes(),
        }
    }

    #[inline]
    pub fn from_f64(val: f64) -> Self {
        Self {
            raw_bytes: val.to_bits().to_le_bytes(),
        }
    }

    #[inline]
    pub fn as_bool(&self) -> bool {
        unsafe { self.raw_bytes[0] != 0 }
    }

    #[inline]
    pub fn as_u8(&self) -> u8 {
        unsafe { self.raw_bytes[0] }
    }

    #[inline]
    pub fn as_i32(&self) -> i32 {
        unsafe { self.i32_val }
    }

    #[inline]
    pub fn as_u32(&self) -> u32 {
        unsafe { self.u32_val }
    }

    #[inline]
    pub fn as_f32(&self) -> f32 {
        unsafe { self.f32_val }
    }

    #[inline]
    pub fn as_f64(&self) -> f64 {
        unsafe { self.f64_val }
    }
}

/// OPC DA / IEC 61850 compliant 16-bit quality status word.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct QualityCode(pub u16);

impl QualityCode {
    pub const BAD_NON_SPECIFIC: Self = Self(0x0000);
    pub const BAD_CONFIG_ERROR: Self = Self(0x0004);
    pub const BAD_NOT_CONNECTED: Self = Self(0x0008);
    pub const BAD_DEVICE_FAILURE: Self = Self(0x000C);
    pub const BAD_SENSOR_FAILURE: Self = Self(0x0010);
    pub const BAD_COMM_FAILURE: Self = Self(0x0018);
    pub const UNCERTAIN_SUBSTITUTE: Self = Self(0x0044);
    pub const UNCERTAIN_INITIAL_VALUE: Self = Self(0x0048);
    pub const GOOD: Self = Self(0x00C0);

    #[inline(always)]
    pub fn is_good(&self) -> bool {
        (self.0 & 0x00C0) == 0x00C0
    }

    #[inline(always)]
    pub fn is_bad(&self) -> bool {
        (self.0 & 0x00C0) == 0x0000
    }

    #[inline(always)]
    pub fn is_uncertain(&self) -> bool {
        (self.0 & 0x00C0) == 0x0040
    }
}

unsafe impl Zeroable for QualityCode {}
unsafe impl Pod for QualityCode {}

/// Tag retention and access permission bit flags.
pub struct TagFlags;
impl TagFlags {
    pub const ACCESS_RO: u32 = 1 << 0;
    pub const ACCESS_WO: u32 = 1 << 1;
    pub const ACCESS_RW: u32 = 1 << 2;

    pub const RETENTION_VOLATILE: u32 = 1 << 4;
    pub const RETENTION_RETAIN: u32 = 1 << 5;
    pub const RETENTION_PERSISTENT: u32 = 1 << 6;

    pub const SIMULATION_ACTIVE: u32 = 1 << 8;
}

/// Cache-line aligned (64 bytes) Plain Old Data tag slot.
#[derive(Copy, Clone, Debug)]
#[repr(C, align(64))]
pub struct TagSlot {
    /// Global unique 32-bit tag identifier in SHM catalog
    pub tag_id: u32,

    /// OPC / IEC data type code (1=BOOL, 2=INT, 3=DINT, 4=REAL, 5=LREAL, etc.)
    pub type_code: u16,

    /// Quality bits (IEC 61850 / OPC DA standard)
    pub quality: QualityCode,

    /// Raw 64-bit value union (8 bytes)
    pub value: TagValueRaw,

    /// Nanoseconds since UNIX Epoch (UTC)
    pub timestamp_ns: u64,

    /// Sequence counter incremented by writer on each update
    pub sequence: u64,

    /// Retention and Access Mode Flags
    pub flags: u32,

    /// Reserved padding to align struct strictly to 64 bytes (1 cache line)
    pub _padding: [u8; 28],
}

unsafe impl Zeroable for TagSlot {}
unsafe impl Pod for TagSlot {}

const _: () = assert!(std::mem::size_of::<TagSlot>() == 64);
const _: () = assert!(std::mem::align_of::<TagSlot>() == 64);

impl Default for TagSlot {
    fn default() -> Self {
        Self {
            tag_id: 0,
            type_code: 0,
            quality: QualityCode::BAD_NOT_CONNECTED,
            value: TagValueRaw {
                raw_bytes: [0u8; 8],
            },
            timestamp_ns: 0,
            sequence: 0,
            flags: TagFlags::ACCESS_RO | TagFlags::RETENTION_VOLATILE,
            _padding: [0u8; 28],
        }
    }
}
