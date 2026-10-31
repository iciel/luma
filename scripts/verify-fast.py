"""Check the fast preset in a downloaded CLI; never builds Rust or runs in CI."""

import argparse
import hashlib
import json
import struct
import subprocess
import time
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def chunks(data):
    assert data.startswith(b"\x89PNG\r\n\x1a\n"), "stdout must contain PNG only"
    pos, found = 8, []
    while pos < len(data):
        assert pos + 12 <= len(data)
        length = int.from_bytes(data[pos:pos + 4])
        end = pos + length + 12
        assert end <= len(data)
        kind = data[pos + 4:pos + 8]
        assert zlib.crc32(data[pos + 4:end - 4]) == int.from_bytes(data[end - 4:end])
        found.append((kind, data[pos + 8:end - 4]))
        pos = end
    assert found[0][0] == b"IHDR" and found[-1] == (b"IEND", b"")
    return found


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    exe = args.exe.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    command = [str(exe), "-", "-o", "-", "--preset", "fast"]
    rows = []

    def verify(name, data):
        started = time.perf_counter()
        result = subprocess.run(command, input=data, capture_output=True, timeout=60, check=True)
        report = json.loads(result.stderr)
        original, output = chunks(data), chunks(result.stdout)
        assert original[0][1][:8] == output[0][1][:8], "dimensions changed"
        assert len(result.stdout) <= len(data), "output grew"
        assert report["preset"] == "fast" and report["score"] is None
        assert report["bytes"] == len(result.stdout)
        (args.output / (name + ".png")).write_bytes(result.stdout)
        row = {"name": name, "input_bytes": len(data), "output_bytes": len(result.stdout),
               "seconds": round(time.perf_counter() - started, 3), "report": report,
               "chunks": [kind.decode() for kind, _ in output]}
        rows.append(row)
        print(json.dumps(row), flush=True)
        return result.stdout

    samples = ["icons/twemoji-1f4a1.png", "3d/fluent-cat-face.png", "illustrations/bicycle-diagram-es.png",
               "photos/kodim03.png", "photos-alpha/cup-of-tea.png"]
    for relative in samples:
        source = ROOT / "bench/images" / relative
        data = source.read_bytes()
        verify(source.stem, data)
        assert source.read_bytes() == data, "source modified"

    # Exact two-color RGBA image with partial transparency, text metadata and EXIF orientation.
    raw = (b"\0" + bytes([0, 0, 0, 255, 200, 30, 20, 128]) * 16) * 24
    exif = b"II\x2a\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\x06\x00" + bytes(6)
    data = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 32, 24, 8, 6, 0, 0, 0))
            + chunk(b"eXIf", exif) + chunk(b"tEXt", b"Comment\0remove this")
            + chunk(b"IDAT", zlib.compress(raw, 0)) + chunk(b"IEND", b""))
    output = verify("transparent-metadata", data)
    parts = dict(chunks(output))
    assert b"tEXt" not in parts and parts[b"eXIf"] == exif
    assert parts[b"tRNS"] == bytes([128]) and parts[b"IHDR"][9] == 3
    indices = zlib.decompress(parts[b"IDAT"])
    assert len(indices) == (1 + 4) * 24  # 1-bit palette, 32 pixels/row plus filter byte.

    source = args.output / "中文 空格.png"
    source.write_bytes(data)
    destination = args.output / "中文 输出.png"
    result = subprocess.run([str(exe), str(source), "-o", str(destination), "--preset", "fast"],
                            capture_output=True, timeout=60, check=True)
    assert destination.read_bytes() == output and source.read_bytes() == data
    assert json.loads(result.stdout)["preset"] == "fast"
    for bad in (b"not PNG", data[:-1], data[:40] + bytes([data[40] ^ 1]) + data[41:]):
        result = subprocess.run(command, input=bad, capture_output=True, timeout=10, check=False)
        assert result.returncode != 0 and not result.stdout and result.stderr
    result = subprocess.run(command + ["--target", "90"], input=data, capture_output=True, timeout=10, check=False)
    assert result.returncode != 0 and not result.stdout
    # The retained quality preset must still produce decodable PNG and a scored report.
    sample = (ROOT / "bench/images/photos/kodim03.png").read_bytes()
    result = subprocess.run([str(exe), "-", "-o", "-", "--preset", "quality"],
                            input=sample, capture_output=True, timeout=180, check=True)
    report = json.loads(result.stderr)
    assert report["score"] is not None and report["bytes"] == len(result.stdout)
    assert chunks(result.stdout)[0][1][:8] == chunks(sample)[0][1][:8]
    assert len(result.stdout) <= len(sample)
    (args.output / "quality-kodim03.png").write_bytes(result.stdout)
    for bad_target in ("nan", "inf", "-1", "101"):
        result = subprocess.run([str(exe), "-", "-o", "-", "--target", bad_target],
                                input=sample, capture_output=True, timeout=10)
        assert result.returncode != 0 and not result.stdout
    summary = {"exe_sha256": hashlib.sha256(exe.read_bytes()).hexdigest(), "rows": rows, "checks_passed": True}
    (args.output / "results.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
