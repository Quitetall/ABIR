use abir::ContentId;
use abir_bcs::{
    encode_forensic_tree, raw_content_id, write_forensic_tree_streaming, Bcs2Error, Bcs2FileError,
    Bcs2View, ForensicContentTransform, ForensicEntry, ForensicEntryMetadata, ForensicFileIndex,
    ForensicFileType, ForensicTree, ForensicTreeMetadata, ForensicTreeView, ResourceBounds,
    CAP_ZSTD,
};
use std::cell::Cell;
use std::io::{self, Cursor, Read, Seek, SeekFrom};

const CHUNK_BYTES: usize = 64 * 1024;

fn entry(path: &[u8], content: Option<Vec<u8>>) -> ForensicEntry {
    ForensicEntry {
        path: path.to_vec(),
        file_type: if content.is_some() {
            ForensicFileType::Regular
        } else {
            ForensicFileType::Directory
        },
        mode: 0o644,
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
        content,
        content_transform: None,
    }
}

fn tree() -> ForensicTree {
    let mut directory = entry(b"recording", None);
    directory.mode = 0o755;
    ForensicTree {
        platform: "linux".into(),
        entries: vec![
            directory,
            entry(b"recording/a.edf", Some(vec![1, 2, 3, 4])),
            entry(b"recording/b.edf", Some(vec![9, 8, 7])),
        ],
    }
}

fn write_tree_streaming(tree: &ForensicTree, bounds: ResourceBounds) -> Vec<u8> {
    let metadata = ForensicTreeMetadata::try_from(tree).unwrap();
    let mut output = Cursor::new(Vec::new());
    write_forensic_tree_streaming(
        &mut output,
        &metadata,
        |content_id| {
            let bytes = tree
                .entries
                .iter()
                .filter_map(|entry| entry.content.as_ref())
                .find(|bytes| raw_content_id(bytes) == content_id)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "frame"))?;
            Ok(Cursor::new(bytes))
        },
        bounds,
    )
    .unwrap();
    output.into_inner()
}

fn assert_streaming_matches_in_memory(tree: &ForensicTree, bounds: ResourceBounds) {
    let expected = encode_forensic_tree(tree, bounds).unwrap();
    let actual = write_tree_streaming(tree, bounds);
    assert_eq!(actual, expected);
    ForensicFileIndex::open(&mut Cursor::new(actual), CAP_ZSTD, bounds).unwrap();
}

#[test]
fn streaming_writer_is_byte_identical_to_in_memory_encoder() {
    let tree = tree();
    let metadata = ForensicTreeMetadata::try_from(&tree).unwrap();
    let expected = encode_forensic_tree(&tree, ResourceBounds::default()).unwrap();
    let mut output = Cursor::new(Vec::new());

    let receipt = write_forensic_tree_streaming(
        &mut output,
        &metadata,
        |content_id| {
            let bytes = tree
                .entries
                .iter()
                .filter_map(|entry| entry.content.as_ref())
                .find(|bytes| raw_content_id(bytes) == content_id)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "frame"))?;
            Ok(Cursor::new(bytes))
        },
        ResourceBounds::default(),
    )
    .unwrap();

    assert_eq!(output.get_ref(), &expected);
    assert_eq!(receipt.artifact_len(), expected.len() as u64);
    assert_eq!(receipt.root_content_id(), metadata.content_id().unwrap());

    let mut reader = Cursor::new(output.into_inner());
    let indexed = ForensicFileIndex::open(&mut reader, 0, ResourceBounds::default()).unwrap();
    assert_eq!(indexed.platform(), metadata.platform);
    assert_eq!(indexed.entries(), metadata.entries);
    assert_eq!(indexed.root_content_id(), receipt.root_content_id());
}

#[test]
fn streaming_writer_matches_empty_transformed_duplicate_and_custom_bound_artifacts() {
    let empty = ForensicTree {
        platform: "linux".into(),
        entries: Vec::new(),
    };
    assert_streaming_matches_in_memory(&empty, ResourceBounds::default());

    let mut transformed = entry(b"capture.edf", Some(b"stored bytes".to_vec()));
    transformed.content_transform = Some(ForensicContentTransform {
        capabilities: CAP_ZSTD,
        logical_content_id: raw_content_id(b"logical bytes"),
        logical_len: 13,
    });
    assert_streaming_matches_in_memory(
        &ForensicTree {
            platform: "linux".into(),
            entries: vec![transformed],
        },
        ResourceBounds::default(),
    );

    let duplicate = ForensicTree {
        platform: "linux".into(),
        entries: vec![
            entry(b"a.edf", Some(b"same bytes".to_vec())),
            entry(b"b.edf", Some(b"same bytes".to_vec())),
        ],
    };
    assert_streaming_matches_in_memory(&duplicate, ResourceBounds::default());

    let custom_bounds = ResourceBounds {
        max_catalog_bytes: 4 * 1024,
        max_index_entries: 16,
        max_frame_bytes: 4 * 1024,
        max_generations: 8,
    };
    assert_streaming_matches_in_memory(&tree(), custom_bounds);
}

#[test]
fn duplicate_frames_are_opened_once() {
    let bytes = b"same bytes".to_vec();
    let content_id = raw_content_id(&bytes);
    let tree = ForensicTree {
        platform: "linux".into(),
        entries: vec![
            entry(b"a.edf", Some(bytes.clone())),
            entry(b"b.edf", Some(bytes.clone())),
        ],
    };
    let metadata = ForensicTreeMetadata::try_from(&tree).unwrap();
    let opens = Cell::new(0);
    let mut output = Cursor::new(Vec::new());
    write_forensic_tree_streaming(
        &mut output,
        &metadata,
        |requested| {
            assert_eq!(requested, content_id);
            opens.set(opens.get() + 1);
            Ok(Cursor::new(bytes.clone()))
        },
        ResourceBounds::default(),
    )
    .unwrap();
    assert_eq!(opens.get(), 1);
}

#[test]
fn streaming_writer_rejects_nonempty_or_nonzero_output_before_writing() {
    let metadata = ForensicTreeMetadata::try_from(&tree()).unwrap();
    let mut nonempty = Cursor::new(vec![0xa5]);
    let error = write_forensic_tree_streaming(
        &mut nonempty,
        &metadata,
        |_| Ok(Cursor::new(Vec::new())),
        ResourceBounds::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        Bcs2FileError::Bcs2(Bcs2Error::NonCanonicalLayout)
    ));
    assert_eq!(nonempty.into_inner(), vec![0xa5]);

    let mut nonzero = Cursor::new(Vec::new());
    nonzero.set_position(1);
    let error = write_forensic_tree_streaming(
        &mut nonzero,
        &metadata,
        |_| Ok(Cursor::new(Vec::new())),
        ResourceBounds::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        Bcs2FileError::Bcs2(Bcs2Error::NonCanonicalLayout)
    ));
    assert!(nonzero.into_inner().is_empty());
}

struct GuardedRepeatReader {
    remaining: usize,
    byte: u8,
}

impl Read for GuardedRepeatReader {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        if destination.len() > CHUNK_BYTES {
            return Err(io::Error::other("reader request exceeded streaming chunk"));
        }
        let count = destination.len().min(self.remaining);
        destination[..count].fill(self.byte);
        self.remaining -= count;
        Ok(count)
    }
}

struct GuardedSeekReader<R> {
    inner: R,
}

impl<R: Read> Read for GuardedSeekReader<R> {
    fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        if destination.len() > CHUNK_BYTES {
            return Err(io::Error::other(
                "validator request exceeded streaming chunk",
            ));
        }
        self.inner.read(destination)
    }
}

impl<R: Seek> Seek for GuardedSeekReader<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.inner.seek(position)
    }
}

fn regular_metadata(path: &[u8], content_id: ContentId, content_len: u64) -> ForensicEntryMetadata {
    ForensicEntryMetadata {
        path: path.to_vec(),
        file_type: ForensicFileType::Regular,
        mode: 0o600,
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
        content_id: Some(content_id),
        content_len: Some(content_len),
        stored_form: None,
    }
}

#[test]
fn writer_and_validator_bound_frame_io_to_fixed_chunks() {
    let frame_len = 2 * 1024 * 1024;
    let content_id = raw_content_id(&vec![0x5a; frame_len]);
    let metadata = ForensicTreeMetadata {
        platform: "archive".into(),
        entries: vec![regular_metadata(b"large.edf", content_id, frame_len as u64)],
    };
    let mut file = tempfile::tempfile().unwrap();
    write_forensic_tree_streaming(
        &mut file,
        &metadata,
        |requested| {
            assert_eq!(requested, content_id);
            Ok(GuardedRepeatReader {
                remaining: frame_len,
                byte: 0x5a,
            })
        },
        ResourceBounds::default(),
    )
    .unwrap();

    file.seek(SeekFrom::Start(0)).unwrap();
    let mut guarded_file = GuardedSeekReader { inner: file };
    let indexed = ForensicFileIndex::open(&mut guarded_file, 0, ResourceBounds::default()).unwrap();
    assert_eq!(indexed.entries(), metadata.entries);
    assert_eq!(indexed.artifact().frames().len(), 2);
}

#[test]
fn streaming_writer_rejects_short_or_relabelled_sources() {
    let content_id = raw_content_id(b"abc");
    let metadata = ForensicTreeMetadata {
        platform: "archive".into(),
        entries: vec![regular_metadata(b"source.edf", content_id, 3)],
    };
    for bytes in [b"ab".as_slice(), b"abd".as_slice()] {
        let mut output = Cursor::new(Vec::new());
        let error = write_forensic_tree_streaming(
            &mut output,
            &metadata,
            |_| Ok(Cursor::new(bytes.to_vec())),
            ResourceBounds::default(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            Bcs2FileError::Bcs2(Bcs2Error::FrameIdentityMismatch)
        ));
    }
}

#[test]
fn forensic_file_index_rejects_metadata_index_capability_disagreement() {
    let original = b"original bytes";
    let stored = b"compressed bytes";
    let mut transformed = entry(b"capture.edf", Some(stored.to_vec()));
    transformed.content_transform = Some(ForensicContentTransform {
        capabilities: CAP_ZSTD,
        logical_content_id: raw_content_id(original),
        logical_len: original.len() as u64,
    });
    let tree = ForensicTree {
        platform: "linux".into(),
        entries: vec![transformed],
    };
    let mut bytes = encode_forensic_tree(&tree, ResourceBounds::default()).unwrap();
    let borrowed = Bcs2View::parse(&bytes, CAP_ZSTD, ResourceBounds::default()).unwrap();
    let stored_id = raw_content_id(stored);
    let index_offset = u64::from_le_bytes(bytes[72..80].try_into().unwrap()) as usize;
    let slot = borrowed
        .frames()
        .iter()
        .position(|frame| frame.content_id() == stored_id)
        .unwrap();
    let entry_offset = index_offset + 48 + slot * 128;
    bytes[entry_offset + 82..entry_offset + 90].copy_from_slice(&0_u64.to_le_bytes());

    let error =
        ForensicFileIndex::open(&mut Cursor::new(bytes), CAP_ZSTD, ResourceBounds::default())
            .unwrap_err();
    assert!(matches!(
        error,
        Bcs2FileError::Bcs2(Bcs2Error::FrameIdentityMismatch)
    ));
}

#[test]
fn forensic_file_index_rejects_capability_gated_metadata_frame() {
    let original = b"original bytes";
    let stored = b"compressed bytes";
    let mut transformed = entry(b"capture.edf", Some(stored.to_vec()));
    transformed.content_transform = Some(ForensicContentTransform {
        capabilities: CAP_ZSTD,
        logical_content_id: raw_content_id(original),
        logical_len: original.len() as u64,
    });
    let tree = ForensicTree {
        platform: "linux".into(),
        entries: vec![transformed],
    };
    let mut bytes = encode_forensic_tree(&tree, ResourceBounds::default()).unwrap();
    let borrowed = Bcs2View::parse(&bytes, CAP_ZSTD, ResourceBounds::default()).unwrap();
    let stored_id = raw_content_id(stored);
    let metadata_slot = borrowed
        .frames()
        .iter()
        .position(|frame| frame.content_id() != stored_id)
        .unwrap();
    let index_offset = u64::from_le_bytes(bytes[72..80].try_into().unwrap()) as usize;
    let entry_offset = index_offset + 48 + metadata_slot * 128;
    bytes[entry_offset + 82..entry_offset + 90].copy_from_slice(&CAP_ZSTD.to_le_bytes());

    let borrowed_error =
        ForensicTreeView::parse(&bytes, CAP_ZSTD, ResourceBounds::default()).unwrap_err();
    assert_eq!(borrowed_error, Bcs2Error::FrameIdentityMismatch);

    let error =
        ForensicFileIndex::open(&mut Cursor::new(bytes), CAP_ZSTD, ResourceBounds::default())
            .unwrap_err();
    assert!(matches!(
        error,
        Bcs2FileError::Bcs2(Bcs2Error::FrameIdentityMismatch)
    ));
}
