// SPDX-License-Identifier: AGPL-3.0-or-later
//! ADR 0139 P2 archive slice: exact source restoration through a BCS2 forensic
//! capsule.
//!
//! Reads one source biosignal file, wraps it in a `bcs.forensic.tree.v1`
//! capsule, parses the capsule back from its encoded bytes, restores it into an
//! empty sandbox, and writes the restored file to the output path. Any
//! divergence between the source and restored bytes is therefore observable by
//! the caller as a hash mismatch; this example itself asserts only the
//! structural invariants the capsule guarantees.

use abir_bcs::{
    encode_forensic_tree, restore_forensic_tree_sandboxed, ForensicEntry, ForensicFileType,
    ForensicTree, ForensicTreeView, ResourceBounds, RestoreMode,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn entry(path: &[u8], file_type: ForensicFileType, mode: u32) -> ForensicEntry {
    ForensicEntry {
        path: path.to_vec(),
        file_type,
        mode,
        owner: None,
        timestamps: [None; 4],
        acl: None,
        xattrs: Vec::new(),
        hardlink_target: None,
        symlink_target: None,
        sparse_extents: Vec::new(),
        flags: 0,
        device: None,
        special_type: None,
        content: None,
    }
}

fn sandbox() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = std::env::temp_dir().join(format!(
        "abir-forensic-archive-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&path)?;
    Ok(path)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let input = PathBuf::from(arguments.next().ok_or("missing input path")?);
    let output = PathBuf::from(arguments.next().ok_or("missing output path")?);
    if arguments.next().is_some() {
        return Err("unexpected extra argument".into());
    }
    let source = fs::read(&input)?;
    let name = input
        .file_name()
        .ok_or("input has no filename")?
        .to_string_lossy()
        .into_owned();

    let directory = entry(b"archive", ForensicFileType::Directory, 0o755);
    let mut file = entry(
        format!("archive/{name}").as_bytes(),
        ForensicFileType::Regular,
        0o640,
    );
    file.content = Some(source.clone());
    let mut entries = vec![directory, file];
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let tree = ForensicTree {
        platform: std::env::consts::OS.into(),
        entries,
    };

    let encoded = encode_forensic_tree(&tree, ResourceBounds::default())?;
    let view = ForensicTreeView::parse(&encoded, 0, ResourceBounds::default())?;
    let destination = sandbox()?;
    let report = restore_forensic_tree_sandboxed(&view, &destination, RestoreMode::Exact)?;
    if report.entries_materialized != 2 || !report.omissions.is_empty() {
        return Err("forensic capsule did not restore the archive exactly".into());
    }
    let restored = fs::read(destination.join("archive").join(&name))?;
    fs::write(output, &restored)?;
    fs::remove_dir_all(&destination)?;
    Ok(())
}
