// SPDX-License-Identifier: AGPL-3.0-or-later
//! ADR 0139 P2 archive slice: fail-closed rejection of a damaged capsule.
//!
//! Wraps one source file in a `bcs.forensic.tree.v1` capsule, corrupts the
//! encoded bytes at the requested offset, and then attempts to parse and
//! restore the damaged capsule. Exit status is the evidence: `0` means the
//! damaged capsule was ACCEPTED (a fail-open defect), any nonzero status means
//! it was correctly rejected before materializing anything.

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let input = PathBuf::from(arguments.next().ok_or("missing input path")?);
    let offset: usize = arguments
        .next()
        .ok_or("missing corruption offset")?
        .to_string_lossy()
        .parse()?;
    if arguments.next().is_some() {
        return Err("unexpected extra argument".into());
    }
    let source = fs::read(&input)?;
    let directory = entry(b"archive", ForensicFileType::Directory, 0o755);
    let mut file = entry(b"archive/source.bin", ForensicFileType::Regular, 0o640);
    file.content = Some(source);
    let mut entries = vec![directory, file];
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let tree = ForensicTree {
        platform: std::env::consts::OS.into(),
        entries,
    };

    let mut encoded = encode_forensic_tree(&tree, ResourceBounds::default())?;
    if encoded.is_empty() {
        return Err("capsule encoded to zero bytes".into());
    }
    // Flip one byte inside the capsule. A content-addressed container must
    // refuse the result rather than restore attacker-chosen contents.
    let position = offset % encoded.len();
    encoded[position] ^= 0xff;

    let view = match ForensicTreeView::parse(&encoded, 0, ResourceBounds::default()) {
        Ok(view) => view,
        Err(_) => return Err("damaged capsule rejected at parse".into()),
    };
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let destination = std::env::temp_dir().join(format!(
        "abir-forensic-probe-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&destination)?;
    let outcome = restore_forensic_tree_sandboxed(&view, &destination, RestoreMode::Exact);
    let accepted = outcome.is_ok();
    fs::remove_dir_all(&destination).ok();
    if accepted {
        // Fail-open: the damaged capsule materialized. Report success so the
        // caller records an acceptance, which its gate then fails on.
        return Ok(());
    }
    Err("damaged capsule rejected at restore".into())
}
