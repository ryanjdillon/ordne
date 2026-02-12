pub mod device;
pub mod hasher;
pub mod dedup;
pub mod rmlint;
pub mod rmlint_import;
pub mod scanner;

pub use device::{DeviceInfo, discover_device};
pub use dedup::{refresh_duplicates_for_drive, DedupAlgorithm, DedupRefreshResult};
pub use hasher::{hash_file_md5, hash_file_blake3, verify_hash};
pub use rmlint::{RmlintParser, RmlintLint, RmlintLintType, parse_rmlint_output};
pub use rmlint_import::{import_rmlint_output, RmlintImportOptions, RmlintImportResult};
pub use scanner::{scan_directory, ScanStats, ScanOptions};
