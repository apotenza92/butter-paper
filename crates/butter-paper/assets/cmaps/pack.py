"""Packs an Adobe Unicode CMap (UTF-16 codes to CIDs) into the range table
`pdf_cmaps.rs` reads: "BPCM", a varint range count, then per range the
code-point gap since the previous range, its length, and the zigzag CID delta
from the previous range's end, all as unsigned LEB128 varints.

Usage: python3 -I pack.py <CMap file> <output .bin>
"""

import re
import sys


def code_point(hexcode):
    return ord(bytes.fromhex(hexcode).decode("utf-16-be"))


def ranges(text):
    entries = []
    for block in re.findall(r"begincidrange(.*?)endcidrange", text, re.S):
        for low, high, cid in re.findall(r"<([0-9a-fA-F]+)>\s*<([0-9a-fA-F]+)>\s*(\d+)", block):
            start = code_point(low)
            entries.append((start, code_point(high) - start + 1, int(cid)))
    for block in re.findall(r"begincidchar(.*?)endcidchar", text, re.S):
        for code, cid in re.findall(r"<([0-9a-fA-F]+)>\s*(\d+)", block):
            entries.append((code_point(code), 1, int(cid)))
    entries.sort()
    merged = []
    for start, count, cid in entries:
        last = merged[-1] if merged else None
        if last and last[0] + last[1] == start and last[2] + last[1] == cid:
            last[1] += count
        else:
            merged.append([start, count, cid])
    return merged


def varint(value):
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        out.append(byte | 0x80 if value else byte)
        if not value:
            return bytes(out)


def encode(merged):
    out = bytearray(b"BPCM") + varint(len(merged))
    previous_end = previous_cid = 0
    for start, count, cid in merged:
        delta = cid - previous_cid
        zigzag = delta * 2 if delta >= 0 else -delta * 2 - 1
        out += varint(start - previous_end) + varint(count) + varint(zigzag)
        previous_end, previous_cid = start + count, cid + count
    return bytes(out)


if __name__ == "__main__":
    source, target = sys.argv[1:]
    with open(source, encoding="latin-1") as handle:
        packed = encode(ranges(handle.read()))
    with open(target, "wb") as handle:
        handle.write(packed)
