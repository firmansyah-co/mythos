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

    /// Store an `f32` widened to `f64`.
    ///
    /// Storage is uniform: every type in this union stores its value **widened losslessly to 64
    /// bits** (integers widen to `i64`/`u64`, floats widen to `f64`). `f32 -> f64 -> f32` is exact,
    /// so [`TagValueRaw::as_f32`] round-trips. The previous implementation stored `f64` bits but
    /// read them back as `f32` (the low 4 bytes), which is not an inverse and returned garbage.
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
    pub fn as_i16(&self) -> i16 {
        (self.as_i64()) as i16
    }

    #[inline]
    pub fn as_u16(&self) -> u16 {
        (self.as_u64()) as u16
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
    pub fn as_i64(&self) -> i64 {
        unsafe { self.i64_val }
    }

    #[inline]
    pub fn as_u64(&self) -> u64 {
        unsafe { self.u64_val }
    }

    /// Read the value back as the `f32` that [`TagValueRaw::from_f32`] stored.
    ///
    /// Narrows from the widened `f64`, mirroring how [`TagValueRaw::as_i16`] narrows from `i64`.
    /// An `f32` widened to `f64` and narrowed back is bit-exact, so this is a true inverse.
    #[inline]
    pub fn as_f32(&self) -> f32 {
        unsafe { self.f64_val as f32 }
    }

    #[inline]
    pub fn as_f64(&self) -> f64 {
        unsafe { self.f64_val }
    }

    /// Decode this value as the given declared type.
    ///
    /// This is the only supported way to turn slot bytes into a number: the type is supplied by
    /// the slot's `type_code`, never guessed from the bytes. Reading a slot with an accessor that
    /// does not match how it was written produces a meaningless number, which is why the declared
    /// type exists.
    #[inline]
    pub fn decode_as(&self, data_type: TagDataType) -> DecodedValue {
        match data_type {
            TagDataType::Bool => DecodedValue::Bool(self.as_bool()),
            TagDataType::Byte => DecodedValue::U8(self.as_u8()),
            TagDataType::Int16 => DecodedValue::I16(self.as_i16()),
            TagDataType::Uint16 => DecodedValue::U16(self.as_u16()),
            TagDataType::Int32 => DecodedValue::I32(self.as_i32()),
            TagDataType::Uint32 => DecodedValue::U32(self.as_u32()),
            TagDataType::Int64 => DecodedValue::I64(self.as_i64()),
            TagDataType::Uint64 => DecodedValue::U64(self.as_u64()),
            TagDataType::Float32 => DecodedValue::F32(self.as_f32()),
            TagDataType::Float64 => DecodedValue::F64(self.as_f64()),
        }
    }
}

/// Declared data type of a tag's value, from [`TagSlot::type_code`].
///
/// The numeric codes are the OPC-style codes the slot layout documents (`1 = BOOL`, `2 = INT`,
/// `3 = DINT`, `4 = REAL`, `5 = LREAL`), extended with the unsigned and 64-bit codes below.
/// Codes `0` and any unlisted value mean **unspecified**: a writer either did not declare a type
/// or used one this build does not know.
///
/// `0` is deliberately *not* aliased to `Float64`. The previous telemetry path read every slot as
/// `f64`, so an integer tag written without a type code surfaced as a denormal and looked like a
/// broken instrument rather than a configuration omission. [`TagDataType::from_code`] therefore
/// returns `None` for `0` so callers must decide explicitly what to do with an undeclared type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum TagDataType {
    /// `i8`/`u8` byte value (OPC BYTE).
    Byte = 10,
    /// `bool` (OPC BOOL).
    Bool = 1,
    /// `i16` (OPC INT).
    Int16 = 2,
    /// `i32` (OPC DINT).
    Int32 = 3,
    /// `f32` (OPC REAL).
    Float32 = 4,
    /// `f64` (OPC LREAL).
    Float64 = 5,
    /// `u16` (OPC UINT).
    Uint16 = 6,
    /// `u32` (OPC UDINT).
    Uint32 = 7,
    /// `i64` (OPC LINT).
    Int64 = 8,
    /// `u64` (OPC ULINT).
    Uint64 = 9,
}

impl TagDataType {
    /// Map a raw `type_code` from a slot. `None` means unspecified or unrecognised.
    pub const fn from_code(code: u16) -> Option<Self> {
        let parsed = match code {
            1 => Self::Bool,
            2 => Self::Int16,
            3 => Self::Int32,
            4 => Self::Float32,
            5 => Self::Float64,
            6 => Self::Uint16,
            7 => Self::Uint32,
            8 => Self::Int64,
            9 => Self::Uint64,
            10 => Self::Byte,
            _ => return None,
        };
        Some(parsed)
    }

    /// The raw `type_code` a writer should pass to store a value of this type.
    pub const fn code(self) -> u16 {
        self as u16
    }

    /// Stable wire name, used wherever the type crosses a process or language boundary.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Byte => "BYTE",
            Self::Bool => "BOOL",
            Self::Int16 => "INT",
            Self::Int32 => "DINT",
            Self::Float32 => "REAL",
            Self::Float64 => "LREAL",
            Self::Uint16 => "UINT",
            Self::Uint32 => "UDINT",
            Self::Int64 => "LINT",
            Self::Uint64 => "ULINT",
        }
    }
}

/// A slot value decoded according to its declared type.
///
/// Kept free of any serialisation concern so `mythos-shm` stays a pure layout crate; the API layer
/// maps it to JSON.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DecodedValue {
    Bool(bool),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
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
