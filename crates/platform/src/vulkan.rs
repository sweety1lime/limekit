//! The bundled libmpv-2.dll and ffmpeg.exe import vulkan-1.dll, which Windows
//! does not ship: a graphics driver with Vulkan support installs it into
//! System32. LimeKit carries the Khronos loader in a `vulkan` folder next to its
//! programs for the machines that have none (old Intel graphics, virtual
//! machines, the Microsoft Basic Display Adapter) and uses it only there, so a
//! driver's own loader always wins.

use std::ffi::OsString;
use std::path::PathBuf;

/// Folder next to the executables that holds the bundled loader.
pub const BUNDLED_DIR: &str = "vulkan";
const LOADER: &str = "vulkan-1.dll";

/// Which vulkan-1.dll the libraries of this installation resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loader {
    /// The graphics driver's copy in System32.
    System,
    /// The copy bundled with LimeKit.
    Bundled(PathBuf),
    /// Neither, so libmpv and ffmpeg cannot start.
    Missing,
}

impl std::fmt::Display for Loader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Loader::System => f.write_str("system"),
            Loader::Bundled(path) => write!(f, "bundled ({})", path.display()),
            Loader::Missing => f.write_str("missing"),
        }
    }
}

/// Finds the loader that would be used, without loading anything.
pub fn locate() -> Loader {
    if system_loader().is_some_and(|path| path.is_file()) {
        return Loader::System;
    }
    match bundled_loader() {
        Some(path) => Loader::Bundled(path),
        None => Loader::Missing,
    }
}

/// Makes vulkan-1.dll resolvable for libraries this process loads afterwards.
/// A system loader is left alone. Otherwise the bundled one is loaded now and
/// stays loaded: Windows reuses an already loaded module of the same name for
/// every later import of it, libmpv's included.
pub fn ensure_loaded() -> Loader {
    let loader = locate();
    if let Loader::Bundled(path) = &loader
        && !load(path)
    {
        return Loader::Missing;
    }
    loader
}

/// PATH for a child process (ffmpeg) with the bundled loader's folder
/// appended. The DLL search reaches PATH only after System32, so a system
/// loader still wins. `None` when this installation bundles no loader.
pub fn child_path() -> Option<OsString> {
    let dir = bundled_loader()?.parent()?.to_path_buf();
    let mut paths: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    paths.push(dir);
    std::env::join_paths(paths).ok()
}

fn bundled_loader() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.parent()?.join(BUNDLED_DIR).join(LOADER);
    path.is_file().then_some(path)
}

fn system_loader() -> Option<PathBuf> {
    let root = std::env::var_os("SystemRoot")?;
    Some(PathBuf::from(root).join("System32").join(LOADER))
}

#[cfg(windows)]
fn load(path: &std::path::Path) -> bool {
    use windows::Win32::System::LibraryLoader::LoadLibraryW;
    use windows::core::HSTRING;
    // SAFETY: loads the Khronos loader shipped with LimeKit; the handle is
    // never freed, which is what keeps it in place for libmpv's imports.
    unsafe { LoadLibraryW(&HSTRING::from(path)) }.is_ok()
}

#[cfg(not(windows))]
fn load(_path: &std::path::Path) -> bool {
    false
}
