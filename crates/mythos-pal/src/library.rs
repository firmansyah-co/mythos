use crate::error::PalError;
use crate::traits::PlatformLibrary;
use std::path::Path;

/// Cross-platform dynamic library wrapper backed by `libloading`.
#[derive(Debug)]
pub struct DynamicLibrary {
    lib: libloading::Library,
    path: String,
}

unsafe impl Send for DynamicLibrary {}
unsafe impl Sync for DynamicLibrary {}

impl DynamicLibrary {
    /// Return the file path from which this dynamic library was loaded.
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl PlatformLibrary for DynamicLibrary {
    fn load<P: AsRef<Path>>(path: P) -> Result<Self, PalError> {
        let path_ref = path.as_ref();
        let path_str = path_ref.to_string_lossy().to_string();

        unsafe {
            libloading::Library::new(path_ref)
                .map(|lib| Self {
                    lib,
                    path: path_str.clone(),
                })
                .map_err(|e| PalError::LibraryLoadFailed {
                    path: path_str,
                    reason: e.to_string(),
                })
        }
    }

    unsafe fn get_symbol<T>(&self, symbol_name: &str) -> Result<*const T, PalError> {
        let symbol_bytes = symbol_name.as_bytes();
        let sym: libloading::Symbol<*const T> =
            self.lib
                .get(symbol_bytes)
                .map_err(|e| PalError::SymbolLookupFailed {
                    symbol: symbol_name.to_string(),
                    reason: e.to_string(),
                })?;
        Ok(*sym)
    }
}
