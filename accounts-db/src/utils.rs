use std::path::{Path, PathBuf};
use std::{fs, io};

/// Creates `directory` if it does not exist, and canonicalize its path.
///
/// impl AsRef<Path> is a common Rust pattern for writing functions that work with file paths in a
/// generic and user-friendly way.
/// - It allows the function to accept any type that can be converted into a &Path.
///   This includes common types like &str, String, and PathBuf.
/// - You can call the function with different types of path representations without needing to
///   manually convert them. For example
///
/// All of these calls are valid
/// create_and_canonicalize_directory("/tmp/some_dir");
/// create_and_canonicalize_directory(String::from("/tmp/some_dir"));
/// create_and_canonicalize_directory(PathBuf::from("/tmp/some_dir"));
/// - The function borrows the path instead of taking ownership, which is often more efficient
///   and avoids unnecessary allocations
pub fn create_and_canonicalize_directory(directory: impl AsRef<Path>) -> io::Result<PathBuf> {
    fs::create_dir_all(&directory)?;
    fs::canonicalize(directory)
}
