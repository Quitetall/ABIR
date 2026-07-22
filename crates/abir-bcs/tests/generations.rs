use abir::ContentId;
use abir_bcs::{
    encode_generation_footer, Bcs2Error, GenerationChain, GenerationFooter, GENERATION_FOOTER_LEN,
};

fn append_generation(
    artifact: &mut Vec<u8>,
    generation: u64,
    previous: Option<(u64, [u8; 32])>,
    seed: u8,
) -> u64 {
    let catalog_offset = artifact.len() as u64;
    artifact.extend_from_slice(&[seed; 7]);
    let index_offset = artifact.len() as u64;
    artifact.extend_from_slice(&[seed.wrapping_add(1); 5]);
    let footer_offset = artifact.len() as u64;
    let (previous_offset, previous_digest) = previous.unwrap_or((0, [0; 32]));
    let footer = encode_generation_footer(
        artifact,
        GenerationFooter {
            generation,
            previous_offset,
            previous_digest,
            catalog_offset,
            catalog_len: 7,
            index_offset,
            index_len: 5,
            root_content_id: ContentId::from_bytes([seed; 32]),
            digest: [0; 32],
        },
    )
    .expect("encode footer");
    artifact.extend_from_slice(&footer);
    footer_offset
}

#[test]
fn generation_chain_is_bounded_and_hash_linked() {
    let mut artifact = vec![0xA5; 16];
    let first_offset = append_generation(&mut artifact, 0, None, 1);
    let first = GenerationFooter::parse(&artifact, first_offset).expect("first footer");
    let second_offset = append_generation(&mut artifact, 1, Some((first_offset, first.digest)), 2);
    let chain = GenerationChain::parse(&artifact, second_offset, 2).expect("chain");
    assert_eq!(chain.newest_first().len(), 2);
    assert_eq!(chain.newest_first()[0].generation, 1);
    assert_eq!(chain.newest_first()[1].generation, 0);
    assert_eq!(
        GenerationChain::parse(&artifact, second_offset, 1),
        Err(Bcs2Error::BoundsExceeded)
    );
}

#[test]
fn corruption_truncation_and_false_previous_digest_fail_closed() {
    let mut artifact = vec![0xA5; 16];
    let first_offset = append_generation(&mut artifact, 0, None, 3);
    let first = GenerationFooter::parse(&artifact, first_offset).unwrap();
    let second_offset = append_generation(&mut artifact, 1, Some((first_offset, first.digest)), 4);

    let mut corrupt = artifact.clone();
    corrupt[16] ^= 1;
    assert_eq!(
        GenerationChain::parse(&corrupt, second_offset, 2),
        Err(Bcs2Error::CatalogDigestMismatch)
    );
    assert!(GenerationChain::parse(
        &artifact[..artifact.len() - GENERATION_FOOTER_LEN / 2],
        second_offset,
        2
    )
    .is_err());

    let mut false_link = artifact[..second_offset as usize].to_vec();
    let bad_offset = append_generation(&mut false_link, 1, Some((first_offset, [9; 32])), 5);
    assert_eq!(
        GenerationChain::parse(&false_link, bad_offset, 2),
        Err(Bcs2Error::CatalogDigestMismatch)
    );
}

#[test]
fn writer_rejects_noncanonical_generation_extents() {
    let artifact = vec![0_u8; 32];
    let result = encode_generation_footer(
        &artifact,
        GenerationFooter {
            generation: 0,
            previous_offset: 0,
            previous_digest: [0; 32],
            catalog_offset: 0,
            catalog_len: 15,
            index_offset: 16,
            index_len: 16,
            root_content_id: ContentId::from_bytes([1; 32]),
            digest: [0; 32],
        },
    );
    assert_eq!(result, Err(Bcs2Error::NonCanonicalLayout));

    let overlap = encode_generation_footer(
        &[0_u8; 200],
        GenerationFooter {
            generation: 1,
            previous_offset: 16,
            previous_digest: [1; 32],
            catalog_offset: 100,
            catalog_len: 50,
            index_offset: 150,
            index_len: 50,
            root_content_id: ContentId::from_bytes([2; 32]),
            digest: [0; 32],
        },
    );
    assert_eq!(overlap, Err(Bcs2Error::NonCanonicalLayout));
}
