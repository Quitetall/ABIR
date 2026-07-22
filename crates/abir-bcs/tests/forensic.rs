use abir_bcs::{
    encode_forensic_tree, Bcs2Error, ForensicEntry, ForensicFileType, ForensicTimestamp,
    ForensicTree, ForensicTreeView, ForensicXattr, ResourceBounds, SparseExtent,
};

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

fn representative_tree() -> ForensicTree {
    let mut directory = entry(b"recording", ForensicFileType::Directory);
    directory.owner = Some((1000, 1000));
    directory.timestamps[1] = Some(ForensicTimestamp {
        seconds: 1_721_600_000,
        nanoseconds: 123,
    });
    directory.xattrs.push(ForensicXattr {
        name: b"user.source".to_vec(),
        value: b"EDF".to_vec(),
    });

    let mut regular = entry(b"recording/raw.edf", ForensicFileType::Regular);
    regular.mode = 0o640;
    regular.content = Some(vec![1, 2, 0, 0, 5, 6]);
    regular.sparse_extents = vec![
        SparseExtent {
            offset: 0,
            length: 2,
            is_hole: false,
        },
        SparseExtent {
            offset: 2,
            length: 2,
            is_hole: true,
        },
        SparseExtent {
            offset: 4,
            length: 2,
            is_hole: false,
        },
    ];
    let mut hardlink = entry(b"recording/raw-copy.edf", ForensicFileType::Hardlink);
    hardlink.mode = regular.mode;
    hardlink.hardlink_target = Some(b"recording/raw.edf".to_vec());
    let mut symlink = entry(b"recording/raw-current.edf", ForensicFileType::Symlink);
    symlink.symlink_target = Some(b"raw.edf".to_vec());

    ForensicTree {
        platform: "linux".into(),
        entries: vec![directory, hardlink, regular, symlink],
    }
}

#[test]
fn forensic_tree_is_deterministic_and_payloads_are_zero_copy() {
    let mut tree = representative_tree();
    tree.entries
        .sort_by(|left, right| left.path.cmp(&right.path));
    let bounds = ResourceBounds::default();
    let first = encode_forensic_tree(&tree, bounds).unwrap();
    let second = encode_forensic_tree(&tree, bounds).unwrap();
    assert_eq!(first, second);

    let view = ForensicTreeView::parse(&first, 0, bounds).unwrap();
    assert_eq!(view.platform(), "linux");
    assert_eq!(view.entries().len(), 4);
    let regular = view
        .entries()
        .iter()
        .find(|entry| entry.file_type == ForensicFileType::Regular)
        .unwrap();
    let payload = view.content_bytes(regular).unwrap();
    assert_eq!(payload, [1, 2, 0, 0, 5, 6]);
    let artifact_start = first.as_ptr() as usize;
    assert!((artifact_start..artifact_start + first.len()).contains(&(payload.as_ptr() as usize)));
}

#[test]
fn forensic_tree_preserves_special_metadata_and_deduplicates_content() {
    let mut tree = representative_tree();
    let mut duplicate = entry(b"recording/second.edf", ForensicFileType::Regular);
    duplicate.content = Some(vec![1, 2, 0, 0, 5, 6]);
    tree.entries.insert(3, duplicate);
    let mut device = entry(b"recording/tty", ForensicFileType::CharacterDevice);
    device.device = Some((4, 1));
    tree.entries.push(device);
    let mut unknown = entry(b"recording/vendor-node", ForensicFileType::Unknown);
    unknown.special_type = Some(b"vendor.example".to_vec());
    tree.entries.push(unknown);
    tree.entries
        .sort_by(|left, right| left.path.cmp(&right.path));

    let encoded = encode_forensic_tree(&tree, ResourceBounds::default()).unwrap();
    let view = ForensicTreeView::parse(&encoded, 0, ResourceBounds::default()).unwrap();
    let regular_ids: Vec<_> = view
        .entries()
        .iter()
        .filter(|entry| entry.file_type == ForensicFileType::Regular)
        .map(|entry| entry.content_id.unwrap())
        .collect();
    assert_eq!(regular_ids[0], regular_ids[1]);
    assert_eq!(view.artifact().frames().len(), 2);
}

#[test]
fn unsafe_paths_and_semantically_invalid_entries_fail_closed() {
    let bounds = ResourceBounds::default();
    for path in [b"/absolute".as_slice(), b"../escape", b"a//b", b"a/./b"] {
        let mut regular = entry(path, ForensicFileType::Regular);
        regular.content = Some(Vec::new());
        let tree = ForensicTree {
            platform: "linux".into(),
            entries: vec![regular],
        };
        assert_eq!(
            encode_forensic_tree(&tree, bounds),
            Err(Bcs2Error::SemanticEncoding)
        );
    }

    let mut regular = entry(b"raw.edf", ForensicFileType::Regular);
    regular.content = Some(vec![1, 2]);
    regular.timestamps[0] = Some(ForensicTimestamp {
        seconds: 0,
        nanoseconds: 1_000_000_000,
    });
    let tree = ForensicTree {
        platform: "linux".into(),
        entries: vec![regular],
    };
    assert_eq!(
        encode_forensic_tree(&tree, bounds),
        Err(Bcs2Error::SemanticEncoding)
    );
}
