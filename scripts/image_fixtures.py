"""Small PNG fixtures and structural checks shared by the executable checks."""

import struct
import zlib

SIGNATURE = b"\x89PNG\r\n\x1a\n"


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def chunks(data):
    assert data.startswith(SIGNATURE), "stdout must contain PNG only"
    pos, found = 8, []
    while pos < len(data):
        assert pos + 12 <= len(data)
        length = int.from_bytes(data[pos:pos + 4], "big")
        end = pos + length + 12
        assert end <= len(data)
        kind = data[pos + 4:pos + 8]
        assert zlib.crc32(data[pos + 4:end - 4]) == int.from_bytes(data[end - 4:end], "big")
        found.append((kind, data[pos + 8:end - 4]))
        pos = end
    assert found[0][0] == b"IHDR" and found[-1] == (b"IEND", b"")
    return found


def solid_png(width=3200, height=2501):
    """A valid 8,003,200-pixel PNG by default, kept under the 64 MiB stdin limit."""
    row = b"\0" + b"\x20\x60\xa0" * width
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return SIGNATURE + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(row * height, 0)) + chunk(b"IEND", b"")


def animated_png():
    """Two distinct full frames, exact black/white palette, 100/200 ms, two loops."""
    width, height = 32, 24
    result = SIGNATURE + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
    result += chunk(b"acTL", struct.pack(">II", 2, 2))
    for frame in range(2):
        control = struct.pack(">IIIIIHHBB", frame, width, height, 0, 0, frame + 1, 10, 0, 0)
        result += chunk(b"fcTL", control)
        row = b"\0" + (b"\0\0\0" * 16 + b"\xff\xff\xff" * 16 if frame == 0
                       else b"\xff\xff\xff" * 16 + b"\0\0\0" * 16)
        encoded = zlib.compress(row * height, 0)
        result += chunk(b"IDAT", encoded) if frame == 0 else chunk(b"fdAT", struct.pack(">I", 2) + encoded)
    return result + chunk(b"IEND", b"")
