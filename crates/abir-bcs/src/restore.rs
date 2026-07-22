use crate::forensic::validate_restore_path;
use crate::{Bcs2Error, ForensicEntryMetadata, ForensicFileType, ForensicTreeView, SparseExtent};
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestoreMode {
    Exact,
    Portable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreOmission {
    pub path: Vec<u8>,
    pub feature: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestoreReport {
    pub entries_materialized: usize,
    pub omissions: Vec<RestoreOmission>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum RestoreError {
    Wire(Bcs2Error),
    DestinationMissing,
    DestinationNotDirectory,
    DestinationNotEmpty,
    PlatformMismatch {
        recorded: String,
        host: &'static str,
    },
    UnsupportedExactMetadata {
        path: Vec<u8>,
        feature: &'static str,
    },
    UnsupportedNode {
        path: Vec<u8>,
        file_type: ForensicFileType,
    },
    UnsafeSymlink {
        path: Vec<u8>,
    },
    MissingContent {
        path: Vec<u8>,
    },
    NonPortablePath {
        path: Vec<u8>,
    },
    Io {
        operation: &'static str,
        kind: std::io::ErrorKind,
    },
}

impl fmt::Display for RestoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "forensic restore error: {self:?}")
    }
}

impl std::error::Error for RestoreError {}

impl From<Bcs2Error> for RestoreError {
    fn from(value: Bcs2Error) -> Self {
        Self::Wire(value)
    }
}

pub fn restore_forensic_tree_sandboxed(
    tree: &ForensicTreeView<'_>,
    destination: &Path,
    mode: RestoreMode,
) -> Result<RestoreReport, RestoreError> {
    preflight_destination(destination)?;
    let omissions = preflight_tree(tree, mode)?;

    let mut directories = Vec::new();
    for entry in tree.entries() {
        if entry.file_type == ForensicFileType::Directory {
            let path = destination.join(path_component(&entry.path)?);
            fs::create_dir(&path).map_err(|error| io("create directory", error))?;
            directories.push((path, entry.mode));
        }
    }
    for entry in tree.entries() {
        if entry.file_type == ForensicFileType::Regular {
            restore_regular(tree, destination, entry, mode)?;
        }
    }
    for entry in tree.entries() {
        if entry.file_type == ForensicFileType::Hardlink {
            restore_hardlink(destination, entry)?;
        }
    }
    for entry in tree.entries() {
        if entry.file_type == ForensicFileType::Symlink {
            restore_symlink(destination, entry)?;
        }
    }
    if mode == RestoreMode::Exact {
        for (path, mode) in directories.into_iter().rev() {
            set_mode(&path, mode)?;
        }
    }
    Ok(RestoreReport {
        entries_materialized: tree.entries().len(),
        omissions,
    })
}

fn preflight_destination(destination: &Path) -> Result<(), RestoreError> {
    let metadata = fs::symlink_metadata(destination).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            RestoreError::DestinationMissing
        } else {
            io("inspect destination", error)
        }
    })?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(RestoreError::DestinationNotDirectory);
    }
    if fs::read_dir(destination)
        .map_err(|error| io("read destination", error))?
        .next()
        .is_some()
    {
        return Err(RestoreError::DestinationNotEmpty);
    }
    Ok(())
}

fn preflight_tree(
    tree: &ForensicTreeView<'_>,
    mode: RestoreMode,
) -> Result<Vec<RestoreOmission>, RestoreError> {
    if mode == RestoreMode::Exact && tree.platform() != std::env::consts::OS {
        return Err(RestoreError::PlatformMismatch {
            recorded: tree.platform().into(),
            host: std::env::consts::OS,
        });
    }
    let mut omissions = Vec::new();
    for entry in tree.entries() {
        path_component(&entry.path)?;
        if !matches!(
            entry.file_type,
            ForensicFileType::Regular
                | ForensicFileType::Directory
                | ForensicFileType::Hardlink
                | ForensicFileType::Symlink
        ) {
            return Err(RestoreError::UnsupportedNode {
                path: entry.path.clone(),
                file_type: entry.file_type,
            });
        }
        if entry.file_type == ForensicFileType::Symlink {
            preflight_symlink(entry)?;
        }
        for (present, feature) in [
            (entry.owner.is_some(), "ownership"),
            (entry.timestamps.iter().any(Option::is_some), "timestamps"),
            (entry.acl.is_some(), "acl"),
            (!entry.xattrs.is_empty(), "xattrs"),
            (entry.flags != 0, "filesystem flags"),
        ] {
            if present {
                if mode == RestoreMode::Exact {
                    return Err(RestoreError::UnsupportedExactMetadata {
                        path: entry.path.clone(),
                        feature,
                    });
                }
                omissions.push(RestoreOmission {
                    path: entry.path.clone(),
                    feature,
                });
            }
        }
        if mode == RestoreMode::Exact
            && entry.file_type == ForensicFileType::Symlink
            && entry.mode != 0o777
        {
            return Err(RestoreError::UnsupportedExactMetadata {
                path: entry.path.clone(),
                feature: "symlink mode",
            });
        }
    }
    Ok(omissions)
}

#[cfg(unix)]
fn preflight_symlink(entry: &ForensicEntryMetadata) -> Result<(), RestoreError> {
    let target = entry
        .symlink_target
        .as_deref()
        .ok_or_else(|| RestoreError::UnsafeSymlink {
            path: entry.path.clone(),
        })?;
    if validate_restore_path(target).is_err() {
        return Err(RestoreError::UnsafeSymlink {
            path: entry.path.clone(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn preflight_symlink(entry: &ForensicEntryMetadata) -> Result<(), RestoreError> {
    Err(RestoreError::UnsupportedNode {
        path: entry.path.clone(),
        file_type: entry.file_type,
    })
}

fn restore_regular(
    tree: &ForensicTreeView<'_>,
    destination: &Path,
    entry: &ForensicEntryMetadata,
    mode: RestoreMode,
) -> Result<(), RestoreError> {
    let content = tree
        .content_bytes(entry)
        .ok_or_else(|| RestoreError::MissingContent {
            path: entry.path.clone(),
        })?;
    let path = destination.join(path_component(&entry.path)?);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| io("create file", error))?;
    write_sparse(&mut file, content, &entry.sparse_extents)?;
    file.sync_all().map_err(|error| io("sync file", error))?;
    if mode == RestoreMode::Exact {
        set_mode(&path, entry.mode)?;
    }
    Ok(())
}

fn write_sparse(
    file: &mut File,
    content: &[u8],
    extents: &[SparseExtent],
) -> Result<(), RestoreError> {
    if extents.is_empty() {
        file.write_all(content)
            .map_err(|error| io("write file", error))?;
        return Ok(());
    }
    file.set_len(content.len() as u64)
        .map_err(|error| io("set sparse file length", error))?;
    for extent in extents.iter().filter(|extent| !extent.is_hole) {
        let start = usize::try_from(extent.offset)
            .map_err(|_| RestoreError::Wire(Bcs2Error::InvalidExtent))?;
        let end_u64 = extent
            .offset
            .checked_add(extent.length)
            .ok_or(RestoreError::Wire(Bcs2Error::InvalidExtent))?;
        let end =
            usize::try_from(end_u64).map_err(|_| RestoreError::Wire(Bcs2Error::InvalidExtent))?;
        let bytes = content
            .get(start..end)
            .ok_or(RestoreError::Wire(Bcs2Error::InvalidExtent))?;
        file.seek(SeekFrom::Start(extent.offset))
            .map_err(|error| io("seek sparse extent", error))?;
        file.write_all(bytes)
            .map_err(|error| io("write sparse extent", error))?;
    }
    Ok(())
}

fn restore_hardlink(destination: &Path, entry: &ForensicEntryMetadata) -> Result<(), RestoreError> {
    let source = entry
        .hardlink_target
        .as_deref()
        .ok_or_else(|| RestoreError::MissingContent {
            path: entry.path.clone(),
        })?;
    fs::hard_link(
        destination.join(path_component(source)?),
        destination.join(path_component(&entry.path)?),
    )
    .map_err(|error| io("create hardlink", error))
}

fn restore_symlink(destination: &Path, entry: &ForensicEntryMetadata) -> Result<(), RestoreError> {
    let target = entry
        .symlink_target
        .as_deref()
        .ok_or_else(|| RestoreError::UnsafeSymlink {
            path: entry.path.clone(),
        })?;
    let target = path_component(target)?;
    let link = destination.join(path_component(&entry.path)?);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).map_err(|error| io("create symlink", error))
    }
    #[cfg(not(unix))]
    {
        let _ = (target, link);
        Err(RestoreError::UnsupportedExactMetadata {
            path: entry.path.clone(),
            feature: "symlink",
        })
    }
}

fn path_component(path: &[u8]) -> Result<PathBuf, RestoreError> {
    validate_restore_path(path)?;
    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        Ok(PathBuf::from(OsStr::from_bytes(path)))
    }
    #[cfg(not(unix))]
    {
        let path = core::str::from_utf8(path).map_err(|_| RestoreError::NonPortablePath {
            path: path.to_vec(),
        })?;
        Ok(PathBuf::from(path))
    }
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), RestoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| io("set mode", error))
}

#[cfg(not(unix))]
fn set_mode(path: &Path, _mode: u32) -> Result<(), RestoreError> {
    Err(RestoreError::UnsupportedExactMetadata {
        path: path.as_os_str().as_encoded_bytes().to_vec(),
        feature: "mode",
    })
}

fn io(operation: &'static str, error: std::io::Error) -> RestoreError {
    RestoreError::Io {
        operation,
        kind: error.kind(),
    }
}
