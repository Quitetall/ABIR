#![cfg(unix)]

use abir_bcs::{
    encode_forensic_tree, restore_forensic_tree_sandboxed, ForensicEntry, ForensicFileType,
    ForensicTree, ForensicTreeView, ResourceBounds, RestoreError, RestoreMode,
};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn entry(path: &[u8], file_type: ForensicFileType) -> ForensicEntry {
    ForensicEntry {
        path: path.to_vec(),
        file_type,
        mode: 0o755,
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

fn sandbox(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("abir-bcs-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

fn tree() -> ForensicTree {
    let directory = entry(b"data", ForensicFileType::Directory);
    let mut file = entry(b"data/raw.edf", ForensicFileType::Regular);
    file.mode = 0o640;
    file.content = Some(b"raw bytes".to_vec());
    let mut hardlink = entry(b"data/raw-copy.edf", ForensicFileType::Hardlink);
    hardlink.mode = file.mode;
    hardlink.hardlink_target = Some(b"data/raw.edf".to_vec());
    let mut symlink = entry(b"data/raw-current.edf", ForensicFileType::Symlink);
    symlink.mode = 0o777;
    symlink.symlink_target = Some(b"raw.edf".to_vec());
    let mut entries = vec![directory, file, hardlink, symlink];
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    ForensicTree {
        platform: std::env::consts::OS.into(),
        entries,
    }
}

#[test]
fn exact_restore_materializes_only_inside_an_empty_sandbox() {
    let encoded = encode_forensic_tree(&tree(), ResourceBounds::default()).unwrap();
    let view = ForensicTreeView::parse(&encoded, 0, ResourceBounds::default()).unwrap();
    let destination = sandbox("exact");
    let report = restore_forensic_tree_sandboxed(&view, &destination, RestoreMode::Exact).unwrap();
    assert_eq!(report.entries_materialized, 4);
    assert!(report.omissions.is_empty());
    assert_eq!(
        fs::read(destination.join("data/raw.edf")).unwrap(),
        b"raw bytes"
    );
    assert_eq!(
        fs::metadata(destination.join("data/raw.edf"))
            .unwrap()
            .ino(),
        fs::metadata(destination.join("data/raw-copy.edf"))
            .unwrap()
            .ino()
    );
    assert_eq!(
        fs::read_link(destination.join("data/raw-current.edf")).unwrap(),
        PathBuf::from("raw.edf")
    );
    fs::remove_dir_all(destination).unwrap();
}

#[test]
fn exact_restore_fails_before_writing_when_metadata_is_unsupported() {
    let mut tree = tree();
    tree.entries[0].owner = Some((1000, 1000));
    let encoded = encode_forensic_tree(&tree, ResourceBounds::default()).unwrap();
    let view = ForensicTreeView::parse(&encoded, 0, ResourceBounds::default()).unwrap();
    let destination = sandbox("fail-closed");
    assert!(matches!(
        restore_forensic_tree_sandboxed(&view, &destination, RestoreMode::Exact),
        Err(RestoreError::UnsupportedExactMetadata {
            feature: "ownership",
            ..
        })
    ));
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    fs::remove_dir_all(destination).unwrap();
}

#[test]
fn restore_rejects_nonempty_destinations_and_escaping_symlinks() {
    let encoded = encode_forensic_tree(&tree(), ResourceBounds::default()).unwrap();
    let view = ForensicTreeView::parse(&encoded, 0, ResourceBounds::default()).unwrap();
    let destination = sandbox("nonempty");
    fs::write(destination.join("keep"), b"user data").unwrap();
    assert_eq!(
        restore_forensic_tree_sandboxed(&view, &destination, RestoreMode::Portable),
        Err(RestoreError::DestinationNotEmpty)
    );
    assert_eq!(fs::read(destination.join("keep")).unwrap(), b"user data");
    fs::remove_dir_all(destination).unwrap();

    let mut tree = tree();
    let symlink = tree
        .entries
        .iter_mut()
        .find(|entry| entry.file_type == ForensicFileType::Symlink)
        .unwrap();
    symlink.symlink_target = Some(b"../../outside".to_vec());
    let encoded = encode_forensic_tree(&tree, ResourceBounds::default()).unwrap();
    let view = ForensicTreeView::parse(&encoded, 0, ResourceBounds::default()).unwrap();
    let destination = sandbox("symlink");
    assert!(matches!(
        restore_forensic_tree_sandboxed(&view, &destination, RestoreMode::Portable),
        Err(RestoreError::UnsafeSymlink { .. })
    ));
    assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
    fs::remove_dir_all(destination).unwrap();
}
