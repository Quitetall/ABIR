//! A forensic capsule may store a transformed encoding of a file without ever
//! letting the transform touch the chain-of-custody claim.
//!
//! The capsule exists to answer one question: *is this the file that was
//! seized?* `content_id` is that answer. If compressing an entry silently
//! redefined `content_id` as the hash of the compressed blob, every check in
//! the format would still pass while the only claim worth making quietly became
//! false. So the logical identity stays primary, and the stored frame's own
//! identity is recorded beside it as a subordinate fact.
//!
//! What is pinned here:
//!
//! 1. Logical identity survives transformation unchanged.
//! 2. A capsule with no transformed entry encodes as v1, byte for byte — the
//!    extension costs no existing artifact its readability.
//! 3. `content_bytes` refuses transformed entries rather than handing back a
//!    compressed blob to a caller that predates the feature.
//! 4. Metadata and index must agree on the capability mask, which is what makes
//!    an index-only edit detectable — the metadata is bound into the root
//!    content id, the index is not.
//! 5. The redundant spellings (zero capabilities, unchanged identity, a
//!    transform on a non-file) are refused, so one tree has one encoding.

use abir_bcs::{
    encode_forensic_tree, raw_content_id, Bcs2Error, Bcs2View, ForensicContentTransform,
    ForensicEntry, ForensicFileType, ForensicTree, ForensicTreeView, ResourceBounds, CAP_ZSTD,
};

/// Stand-in for a real transform: the test needs a byte string that differs from
/// the logical content, not a real compressor.
const STORED_ENCODING: &[u8] = b"<transformed encoding of the file>";
const FILE_BYTES: &[u8] = b"the original file contents, at rest";
const TRANSFORM_PARAMETERS: [u8; 32] = [0; 32];

/// The logical id must be derived exactly as the format derives frame ids —
/// `raw_content_id` is domain-separated, so a bare blake3 of the same bytes is a
/// different id and would silently defeat the "transform changed nothing" check.
fn logical_id() -> abir::ContentId {
    raw_content_id(FILE_BYTES)
}

fn entry(path: &[u8], file_type: ForensicFileType, content: Option<Vec<u8>>) -> ForensicEntry {
    ForensicEntry {
        path: path.to_vec(),
        file_type,
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

fn tree(entries: Vec<ForensicEntry>) -> ForensicTree {
    ForensicTree {
        platform: "linux".into(),
        entries,
    }
}

/// One directory plus one regular file whose content is stored transformed.
fn transformed_tree() -> ForensicTree {
    let mut root = entry(b"recording", ForensicFileType::Directory, None);
    root.mode = 0o755;
    let mut file = entry(
        b"recording/capture.edf",
        ForensicFileType::Regular,
        Some(STORED_ENCODING.to_vec()),
    );
    file.content_transform = Some(ForensicContentTransform {
        capabilities: CAP_ZSTD,
        logical_content_id: logical_id(),
        logical_len: FILE_BYTES.len() as u64,
        parameters: TRANSFORM_PARAMETERS,
    });
    tree(vec![root, file])
}

#[test]
fn transformation_does_not_disturb_the_chain_of_custody_claim() {
    let bytes = encode_forensic_tree(&transformed_tree(), ResourceBounds::default())
        .expect("capsule encodes");
    let view = ForensicTreeView::parse(&bytes, CAP_ZSTD, ResourceBounds::default())
        .expect("capable reader parses");

    let file = view
        .entries()
        .iter()
        .find(|entry| entry.path == b"recording/capture.edf")
        .expect("file entry present");

    // The identity is of the FILE, not of what was stored.
    assert_eq!(file.content_id, Some(logical_id()));
    assert_eq!(file.content_len, Some(FILE_BYTES.len() as u64));

    let stored = file.stored_form.expect("stored form recorded");
    assert_eq!(stored.capabilities, CAP_ZSTD);
    assert_eq!(stored.stored_len, STORED_ENCODING.len() as u64);
    assert_eq!(stored.parameters, TRANSFORM_PARAMETERS);
    assert_ne!(stored.stored_content_id, logical_id());
    assert_eq!(view.stored_bytes(file), Some(STORED_ENCODING));
}

#[test]
fn transform_parameters_round_trip_in_metadata_version_three() {
    let mut tree = transformed_tree();
    let mut parameters = [0_u8; 32];
    parameters[..8].copy_from_slice(b"template");
    tree.entries[1]
        .content_transform
        .as_mut()
        .expect("transform")
        .parameters = parameters;

    let bytes = encode_forensic_tree(&tree, ResourceBounds::default()).expect("capsule encodes");
    let view = ForensicTreeView::parse(&bytes, CAP_ZSTD, ResourceBounds::default())
        .expect("capable reader parses");
    let file = view
        .entries()
        .iter()
        .find(|entry| entry.path == b"recording/capture.edf")
        .expect("file entry present");
    assert_eq!(
        file.stored_form.expect("stored form").parameters,
        parameters
    );

    let entry_frames: Vec<_> = view
        .entries()
        .iter()
        .filter_map(|entry| entry.frame_content_id())
        .collect();
    let metadata_frame = view
        .artifact()
        .frames()
        .iter()
        .find(|frame| !entry_frames.contains(&frame.content_id()))
        .expect("metadata frame present")
        .bytes();
    assert_eq!(&metadata_frame[..2], &[0x83, 0x03]);
}

#[test]
fn a_reader_without_the_capability_is_refused_at_the_envelope() {
    let bytes = encode_forensic_tree(&transformed_tree(), ResourceBounds::default())
        .expect("capsule encodes");
    assert_eq!(
        ForensicTreeView::parse(&bytes, 0, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::UnsupportedCapabilities(CAP_ZSTD)
    );
}

#[test]
fn content_bytes_refuses_to_pass_off_a_transformed_frame_as_the_file() {
    // Every caller written before stored forms existed reads this as "the file".
    // Returning the stored blob would turn a wire extension into silent data
    // corruption at each of them, so it returns None instead.
    let bytes = encode_forensic_tree(&transformed_tree(), ResourceBounds::default())
        .expect("capsule encodes");
    let view = ForensicTreeView::parse(&bytes, CAP_ZSTD, ResourceBounds::default())
        .expect("capable reader parses");
    let file = view
        .entries()
        .iter()
        .find(|entry| entry.path == b"recording/capture.edf")
        .expect("file entry present");

    assert_eq!(view.content_bytes(file), None);
    assert!(view.stored_bytes(file).is_some());
}

#[test]
fn a_capsule_with_no_transform_still_encodes_as_version_one() {
    // The document version is a function of the content, not a global switch.
    // If it were a switch, every capsule written after this change would become
    // unreadable to every reader written before it, for no gain.
    let mut root = entry(b"recording", ForensicFileType::Directory, None);
    root.mode = 0o755;
    let plain = tree(vec![
        root,
        entry(
            b"recording/capture.edf",
            ForensicFileType::Regular,
            Some(FILE_BYTES.to_vec()),
        ),
    ]);
    let bytes = encode_forensic_tree(&plain, ResourceBounds::default()).expect("capsule encodes");

    let view =
        ForensicTreeView::parse(&bytes, 0, ResourceBounds::default()).expect("plain reader parses");
    let file = view
        .entries()
        .iter()
        .find(|entry| entry.path == b"recording/capture.edf")
        .expect("file entry present");
    assert!(file.stored_form.is_none());
    assert_eq!(file.required_capabilities(), 0);
    assert_eq!(view.content_bytes(file), Some(FILE_BYTES));

    // Frames are ordered by content id, so the metadata frame is identified as
    // the one no entry claims rather than by position.
    let entry_frames: Vec<_> = view
        .entries()
        .iter()
        .filter_map(|entry| entry.frame_content_id())
        .collect();
    let metadata_frame = view
        .artifact()
        .frames()
        .iter()
        .find(|frame| !entry_frames.contains(&frame.content_id()))
        .expect("metadata frame present")
        .bytes();
    // CBOR array(3) then u8(1).
    assert_eq!(
        &metadata_frame[..2],
        &[0x83, 0x01],
        "a transform-free capsule must still carry a version-1 metadata document"
    );
}

#[test]
fn a_transform_requiring_nothing_is_refused() {
    // Zero capabilities is exactly the verbatim spelling. Admitting both would
    // give one tree two encodings and break canonicalisation.
    let mut broken = transformed_tree();
    broken.entries[1].content_transform = Some(ForensicContentTransform {
        capabilities: 0,
        logical_content_id: logical_id(),
        logical_len: FILE_BYTES.len() as u64,
        parameters: TRANSFORM_PARAMETERS,
    });
    assert_eq!(
        encode_forensic_tree(&broken, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::SemanticEncoding
    );
}

#[test]
fn a_transform_that_changes_nothing_is_refused() {
    // Claiming a transform while storing the file verbatim is a false statement
    // about the bytes, even though every hash in the capsule would check out.
    let mut broken = transformed_tree();
    broken.entries[1].content = Some(FILE_BYTES.to_vec());
    broken.entries[1].content_transform = Some(ForensicContentTransform {
        capabilities: CAP_ZSTD,
        logical_content_id: logical_id(),
        logical_len: FILE_BYTES.len() as u64,
        parameters: TRANSFORM_PARAMETERS,
    });
    assert_eq!(
        encode_forensic_tree(&broken, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::SemanticEncoding
    );
}

#[test]
fn a_transform_on_a_directory_is_refused() {
    let mut broken = transformed_tree();
    broken.entries[0].content_transform = Some(ForensicContentTransform {
        capabilities: CAP_ZSTD,
        logical_content_id: logical_id(),
        logical_len: 1,
        parameters: TRANSFORM_PARAMETERS,
    });
    assert_eq!(
        encode_forensic_tree(&broken, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::SemanticEncoding
    );
}

#[test]
fn an_index_only_edit_of_the_capability_mask_is_detected() {
    // The metadata is bound into the root content id; the index entry is not.
    // Requiring the two to agree is the whole reason the mask is recorded twice.
    let bytes = encode_forensic_tree(&transformed_tree(), ResourceBounds::default())
        .expect("capsule encodes");
    let plain = Bcs2View::parse(&bytes, CAP_ZSTD, ResourceBounds::default()).expect("parses");
    let transformed_id = plain
        .frames()
        .iter()
        .find(|frame| frame.required_capabilities() == CAP_ZSTD)
        .expect("one frame declares the transform")
        .content_id();

    // Locate that frame's index entry and clear its declared mask.
    let index_offset = u64::from_le_bytes(bytes[72..80].try_into().unwrap()) as usize;
    let mut tampered = bytes.clone();
    let mut cleared = false;
    for slot in 0..plain.frames().len() {
        let entry = index_offset + 48 + slot * 128;
        if bytes[entry..entry + 32] == transformed_id.as_bytes()[..] {
            tampered[entry + 82..entry + 90].copy_from_slice(&0_u64.to_le_bytes());
            cleared = true;
            break;
        }
    }
    assert!(cleared, "index entry for the transformed frame located");

    assert_eq!(
        ForensicTreeView::parse(&tampered, CAP_ZSTD, ResourceBounds::default()).unwrap_err(),
        Bcs2Error::FrameIdentityMismatch
    );
}
