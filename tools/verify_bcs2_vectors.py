#!/usr/bin/env python3
"""Independent, dependency-free structural verifier for BCS2 golden vectors."""

from __future__ import annotations

import hashlib
import json
import struct
import sys
from pathlib import Path


MAGIC = b"ABIRBCS2"
HEADER_LEN = 128
INDEX_MAGIC = b"BCS2IDX\0"
GENERATION_MAGIC = b"BCS2GEN\0"


class VectorError(RuntimeError):
    pass


def u16(data: bytes, offset: int) -> int:
    return struct.unpack_from("<H", data, offset)[0]


def u32(data: bytes, offset: int) -> int:
    return struct.unpack_from("<I", data, offset)[0]


def u64(data: bytes, offset: int) -> int:
    return struct.unpack_from("<Q", data, offset)[0]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise VectorError(message)


def cbor_argument(data: bytes, offset: int, expected_major: int) -> tuple[int, int]:
    require(offset < len(data), "truncated CBOR")
    initial = data[offset]
    require(initial >> 5 == expected_major, "unexpected CBOR major type")
    info = initial & 0x1F
    require(info != 31, "indefinite CBOR is noncanonical")
    if info < 24:
        return info, offset + 1
    widths = {24: 1, 25: 2, 26: 4, 27: 8}
    require(info in widths, "reserved CBOR additional information")
    width = widths[info]
    end = offset + 1 + width
    require(end <= len(data), "truncated CBOR argument")
    value = int.from_bytes(data[offset + 1 : end], "big")
    minima = {1: 24, 2: 256, 4: 65536, 8: 4294967296}
    require(value >= minima[width], "non-minimal CBOR integer")
    return value, end


def cbor_unsigned(data: bytes, offset: int) -> tuple[int, int]:
    return cbor_argument(data, offset, 0)


def cbor_bytes(data: bytes, offset: int) -> tuple[bytes, int]:
    length, offset = cbor_argument(data, offset, 2)
    end = offset + length
    require(end <= len(data), "truncated CBOR byte string")
    return data[offset:end], end


def cbor_array(data: bytes, offset: int) -> tuple[int, int]:
    return cbor_argument(data, offset, 4)


def verify_catalog(catalog: bytes, root_id: bytes) -> None:
    count, offset = cbor_argument(catalog, 0, 5)
    require(count == 3, "base catalog must have three keys")
    key, offset = cbor_unsigned(catalog, offset)
    require(key == 1, "catalog key 1 missing")
    semantic_json, offset = cbor_bytes(catalog, offset)
    json.loads(semantic_json)
    key, offset = cbor_unsigned(catalog, offset)
    require(key == 2, "catalog key 2 missing")
    embedded_root, offset = cbor_bytes(catalog, offset)
    require(embedded_root == root_id, "catalog root differs from envelope")
    key, offset = cbor_unsigned(catalog, offset)
    require(key == 3, "catalog key 3 missing")
    references, offset = cbor_array(catalog, offset)
    prior = None
    for _ in range(references):
        reference, offset = cbor_bytes(catalog, offset)
        require(len(reference) == 32, "reference ContentId length")
        require(prior is None or prior < reference, "references not strictly sorted")
        prior = reference
    require(offset == len(catalog), "trailing catalog bytes")


def verify_plain(name: str, data: bytes) -> None:
    require(len(data) >= HEADER_LEN + 48, f"{name}: too short")
    require(data[:8] == MAGIC, f"{name}: magic")
    require((u16(data, 8), u16(data, 10)) == (2, 0), f"{name}: version")
    require(u32(data, 12) == HEADER_LEN, f"{name}: header length")
    catalog_offset, catalog_len = u64(data, 56), u64(data, 64)
    index_offset, index_len = u64(data, 72), u64(data, 80)
    latest_footer = u64(data, 88)
    require(catalog_offset == HEADER_LEN, f"{name}: noncanonical catalog offset")
    if data[41] == 2:
        require(latest_footer + 160 == len(data), f"{name}: generation footer extent")
        require(data[latest_footer : latest_footer + 8] == GENERATION_MAGIC, f"{name}: footer magic")
        catalog_offset, catalog_len = u64(data, latest_footer + 64), u64(data, latest_footer + 72)
        index_offset, index_len = u64(data, latest_footer + 80), u64(data, latest_footer + 88)
    else:
        require(index_offset + index_len == len(data), f"{name}: trailing bytes")
    require(catalog_offset + catalog_len <= index_offset, f"{name}: catalog extent")
    require(index_offset + index_len <= len(data), f"{name}: index extent")
    index = data[index_offset : index_offset + index_len]
    require(index[:8] == INDEX_MAGIC, f"{name}: index magic")
    frame_count = u32(index, 8)
    require(index_len == 48 + frame_count * 128, f"{name}: index length")
    verify_catalog(data[catalog_offset : catalog_offset + catalog_len], data[96:128])
    prior = None
    for number in range(frame_count):
        entry = index[48 + number * 128 : 48 + (number + 1) * 128]
        content_id = entry[:32]
        require(prior is None or prior < content_id, f"{name}: frame order")
        prior = content_id
        frame_offset, frame_len = u64(entry, 64), u64(entry, 72)
        require(frame_offset + frame_len <= index_offset, f"{name}: frame extent")


def verify_encrypted(name: str, data: bytes) -> None:
    require(len(data) >= 168 and data[:8] == MAGIC, f"{name}: encrypted envelope")
    require(data[41] == 1 and data[42] in (2, 3) and data[43] == 2, f"{name}: privacy fields")
    ciphertext_len = u32(data, 44)
    require(u32(data, 48) == 24 and u32(data, 52) == 16, f"{name}: nonce/tag lengths")
    require(u64(data, 56) == 152 and u64(data, 64) == ciphertext_len, f"{name}: ciphertext extent")
    require(u64(data, 72) == 128 and u64(data, 80) == 24, f"{name}: nonce extent")
    require(152 + ciphertext_len == len(data), f"{name}: encrypted trailing bytes")


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "fixtures/bcs2/v1")
    manifest = json.loads((root / "manifest.json").read_text())
    require(manifest["schema_version"] == 1 and manifest["wire_major"] == 2, "manifest version")
    for vector in manifest["vectors"]:
        name = vector["name"]
        data = (root / name).read_bytes()
        require(len(data) == vector["bytes"], f"{name}: manifest length")
        require(hashlib.sha256(data).hexdigest() == vector["sha256"], f"{name}: sha256")
        if name == "encrypted-discoverable.bcs2":
            verify_encrypted(name, data)
        else:
            verify_plain(name, data)
        print(f"verified {name}")
    print("BCS2 independent vector verification: PASS")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, KeyError, ValueError, VectorError) as error:
        print(f"BCS2 independent vector verification: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
