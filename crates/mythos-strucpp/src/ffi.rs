//! Safe Rust wrappers for C/C++ ABIs, STruC++ descriptors, and dynamic symbol bindings.

use crate::error::StrucppError;
use std::ffi::c_void;

/// Memory area for located variables conforming to `strucpp::LocatedArea`.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LocatedArea {
    /// %I - Input area
    Input = 0,
    /// %Q - Output area
    Output = 1,
    /// %M - Internal memory marker area
    Memory = 2,
}

impl LocatedArea {
    pub fn from_u8(val: u8) -> Result<Self, StrucppError> {
        match val {
            0 => Ok(Self::Input),
            1 => Ok(Self::Output),
            2 => Ok(Self::Memory),
            other => Err(StrucppError::InvalidLocatedDescriptor {
                area: other,
                size: 0,
                byte_index: 0,
                bit_index: 0,
            }),
        }
    }

    pub fn as_char(&self) -> char {
        match self {
            Self::Input => 'I',
            Self::Output => 'Q',
            Self::Memory => 'M',
        }
    }
}

/// Data size for located variables conforming to `strucpp::LocatedSize`.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum LocatedSize {
    /// %?X - Single bit
    Bit = 0,
    /// %?B - Byte (8 bits)
    Byte = 1,
    /// %?W - Word (16 bits)
    Word = 2,
    /// %?D - Double Word (32 bits)
    DWord = 3,
    /// %?L - Long Word (64 bits)
    LWord = 4,
}

impl LocatedSize {
    pub fn from_u8(val: u8) -> Result<Self, StrucppError> {
        match val {
            0 => Ok(Self::Bit),
            1 => Ok(Self::Byte),
            2 => Ok(Self::Word),
            3 => Ok(Self::DWord),
            4 => Ok(Self::LWord),
            other => Err(StrucppError::InvalidLocatedDescriptor {
                area: 0,
                size: other,
                byte_index: 0,
                bit_index: 0,
            }),
        }
    }

    pub fn as_char(&self) -> char {
        match self {
            Self::Bit => 'X',
            Self::Byte => 'B',
            Self::Word => 'W',
            Self::DWord => 'D',
            Self::LWord => 'L',
        }
    }

    pub fn byte_len(&self) -> usize {
        match self {
            Self::Bit => 1,
            Self::Byte => 1,
            Self::Word => 2,
            Self::DWord => 4,
            Self::LWord => 8,
        }
    }
}

/// Exact 16-byte memory layout matching `strucpp::LocatedVar` (64-bit platform).
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct LocatedVarRaw {
    pub area: u8,
    pub size: u8,
    pub byte_index: u16,
    pub bit_index: u8,
    pub _reserved: [u8; 3],
    pub pointer: *mut c_void,
}

// Compile-time static assertions ensuring exact 16-byte size and 8-byte alignment
const _: () = {
    assert!(std::mem::size_of::<LocatedVarRaw>() == 16);
    assert!(std::mem::align_of::<LocatedVarRaw>() == 8);
};

/// High-level safe descriptor for a located IEC variable.
#[derive(Debug, Clone)]
pub struct LocatedVarDescriptor {
    pub area: LocatedArea,
    pub size: LocatedSize,
    pub byte_index: u16,
    pub bit_index: u8,
    pub raw_pointer: usize,
}

impl LocatedVarDescriptor {
    /// Convert from C raw descriptor into verified Rust representation.
    pub fn from_raw(raw: &LocatedVarRaw) -> Result<Self, StrucppError> {
        let area = LocatedArea::from_u8(raw.area)?;
        let size = LocatedSize::from_u8(raw.size)?;
        Ok(Self {
            area,
            size,
            byte_index: raw.byte_index,
            bit_index: raw.bit_index,
            raw_pointer: raw.pointer as usize,
        })
    }

    /// Return canonical IEC address string (e.g. `%IX0.0`, `%QW2`, `%MD100`).
    pub fn iec_address(&self) -> String {
        if self.size == LocatedSize::Bit {
            format!(
                "%{}{}{}.{}",
                self.area.as_char(),
                self.size.as_char(),
                self.byte_index,
                self.bit_index
            )
        } else {
            format!(
                "%{}{}{}",
                self.area.as_char(),
                self.size.as_char(),
                self.byte_index
            )
        }
    }

    /// Read raw boolean value safely.
    ///
    /// # Safety
    /// Caller guarantees the pointer remains valid and points to an `IEC_BOOL` or `bool`.
    pub unsafe fn read_bool(&self) -> bool {
        if self.raw_pointer == 0 {
            return false;
        }
        let ptr = self.raw_pointer as *const u8;
        *ptr != 0
    }

    /// Write raw boolean value safely.
    ///
    /// # Safety
    /// Caller guarantees pointer is writable and represents an `IEC_BOOL`.
    pub unsafe fn write_bool(&self, val: bool) {
        if self.raw_pointer != 0 {
            let ptr = self.raw_pointer as *mut u8;
            *ptr = val as u8;
        }
    }

    /// Read integer value based on size descriptor.
    ///
    /// # Safety
    /// Valid pointer matching `size`.
    pub unsafe fn read_u64(&self) -> u64 {
        if self.raw_pointer == 0 {
            return 0;
        }
        match self.size {
            LocatedSize::Bit => self.read_bool() as u64,
            LocatedSize::Byte => *(self.raw_pointer as *const u8) as u64,
            LocatedSize::Word => *(self.raw_pointer as *const u16) as u64,
            LocatedSize::DWord => *(self.raw_pointer as *const u32) as u64,
            LocatedSize::LWord => *(self.raw_pointer as *const u64),
        }
    }

    /// Write integer value based on size descriptor.
    ///
    /// # Safety
    /// Valid pointer matching `size`.
    pub unsafe fn write_u64(&self, val: u64) {
        if self.raw_pointer == 0 {
            return;
        }
        match self.size {
            LocatedSize::Bit => self.write_bool((val & 1) != 0),
            LocatedSize::Byte => *(self.raw_pointer as *mut u8) = val as u8,
            LocatedSize::Word => *(self.raw_pointer as *mut u16) = val as u16,
            LocatedSize::DWord => *(self.raw_pointer as *mut u32) = val as u32,
            LocatedSize::LWord => *(self.raw_pointer as *mut u64) = val,
        }
    }
}

/// Dynamic symbol bindings for loaded STruC++ dynamic runtime modules.
pub struct StrucppSymbols {
    pub get_config: libloading::Symbol<'static, unsafe extern "C" fn() -> *mut c_void>,
    pub set_locks:
        Option<libloading::Symbol<'static, unsafe extern "C" fn(*mut c_void, *mut c_void)>>,
    pub get_located_vars:
        Option<libloading::Symbol<'static, unsafe extern "C" fn(*mut u32) -> *mut LocatedVarRaw>>,
    pub get_located_vars_count: Option<libloading::Symbol<'static, unsafe extern "C" fn() -> u32>>,
    pub step_cycle: Option<libloading::Symbol<'static, unsafe extern "C" fn()>>,
    pub debug_array_count: Option<libloading::Symbol<'static, unsafe extern "C" fn() -> u8>>,
    pub debug_elem_count: Option<libloading::Symbol<'static, unsafe extern "C" fn(u8) -> u16>>,
    pub debug_size: Option<libloading::Symbol<'static, unsafe extern "C" fn(u8, u16) -> u16>>,
    pub debug_set:
        Option<libloading::Symbol<'static, unsafe extern "C" fn(u8, u16, *const c_void) -> u8>>,
    pub debug_read:
        Option<libloading::Symbol<'static, unsafe extern "C" fn(u8, u16, *mut c_void) -> u16>>,
    pub debug_write:
        Option<libloading::Symbol<'static, unsafe extern "C" fn(u8, u16, *const c_void) -> u8>>,
}

impl StrucppSymbols {
    /// Resolve dynamic symbols from a loaded `libloading::Library`.
    ///
    /// # Safety
    /// The returned symbols are tied to the lifetime of `lib`. Caller must keep `lib` alive
    /// as long as `StrucppSymbols` is used.
    pub unsafe fn resolve(lib: &'static libloading::Library) -> Result<Self, StrucppError> {
        let get_config =
            lib.get(b"strucpp_get_config")
                .map_err(|e| StrucppError::SymbolNotFound {
                    symbol: "strucpp_get_config".into(),
                    reason: e.to_string(),
                })?;

        let set_locks = lib.get(b"strucpp_set_locks").ok();
        let get_located_vars = lib.get(b"strucpp_get_located_vars").ok();
        let get_located_vars_count = lib.get(b"strucpp_get_located_vars_count").ok();
        let step_cycle = lib.get(b"strucpp_step_cycle").ok();
        let debug_array_count = lib.get(b"strucpp_debug_array_count").ok();
        let debug_elem_count = lib.get(b"strucpp_debug_elem_count").ok();
        let debug_size = lib.get(b"strucpp_debug_size").ok();
        let debug_set = lib.get(b"strucpp_debug_set").ok();
        let debug_read = lib.get(b"strucpp_debug_read").ok();
        let debug_write = lib.get(b"strucpp_debug_write").ok();

        Ok(Self {
            get_config,
            set_locks,
            get_located_vars,
            get_located_vars_count,
            step_cycle,
            debug_array_count,
            debug_elem_count,
            debug_size,
            debug_set,
            debug_read,
            debug_write,
        })
    }
}
