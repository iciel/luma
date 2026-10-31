"""Check PNG output in either Luma preset using an existing executable; requires Pillow."""

import argparse
import hashlib
import io
import json
import math
import struct
import subprocess
import time
import zlib
from pathlib import Path

from PIL import Image

from image_fixtures import SIGNATURE, animated_png, chunk, chunks, solid_png

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--preset", choices=["quality", "fast"], default="quality")
    args = parser.parse_args()
    exe = args.exe.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    command = [str(exe), "-", "-o", "-", "--preset", args.preset, "--format", "png"]
    rows = []

    def verify(name, data, *options):
        started = time.perf_counter()
        result = subprocess.run(command + list(options), input=data, capture_output=True, timeout=600)
        assert result.returncode == 0, f"{name}: {result.stderr.decode(errors='replace')}"
        seconds = time.perf_counter() - started
        report = json.loads(result.stderr)
        original, output = chunks(data), chunks(result.stdout)
        assert original[0][1][:8] == output[0][1][:8], "dimensions changed"
        assert len(result.stdout) < len(data), "successful output must be smaller"
        assert report["bytes"] == len(result.stdout)
        if args.preset == "fast":
            assert report["preset"] == "fast" and report["score"] is None
        else:
            assert math.isfinite(report["score"]) and math.isfinite(report["target"])
            assert isinstance(report["miss"], bool) and report["evals"] > 0
        with Image.open(io.BytesIO(result.stdout)) as decoded:
            for frame in range(decoded.n_frames):
                decoded.seek(frame)
                decoded.load()
        (args.output / (name + ".png")).write_bytes(result.stdout)
        rows.append({"name": name, "input_bytes": len(data), "output_bytes": len(result.stdout),
                     "seconds": round(seconds, 3), "report": report,
                     "chunks": [kind.decode() for kind, _ in output]})
        print(json.dumps(rows[-1]), flush=True)
        return result.stdout, report

    def reject(data, message, *options):
        result = subprocess.run(command + list(options), input=data, capture_output=True, timeout=60)
        assert result.returncode != 0 and not result.stdout and message in result.stderr, result.stderr

    for relative in ["3d/fluent-cat-face.png", "illustrations/bicycle-diagram-es.png",
                     "photos/kodim03.png", "photos-alpha/cup-of-tea.png"]:
        source = ROOT / "bench/images" / relative
        data = source.read_bytes()
        verify(source.stem, data)
        assert source.read_bytes() == data, "source modified"

    # An exact two-color image checks actual alpha values independently of palette order.
    raw = (b"\0" + bytes([0, 0, 0, 255, 200, 30, 20, 128]) * 16) * 24
    exif = b"II\x2a\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\x06\x00" + bytes(6)
    data = (SIGNATURE + chunk(b"IHDR", struct.pack(">IIBBBBB", 32, 24, 8, 6, 0, 0, 0))
            + (chunk(b"eXIf", exif) if args.preset == "fast" else b"")
            + chunk(b"tEXt", b"Comment\0remove this")
            + chunk(b"IDAT", zlib.compress(raw, 0)) + chunk(b"IEND", b""))
    output, _ = verify("transparent-metadata", data)
    parts = dict(chunks(output))
    assert b"tEXt" not in parts
    if args.preset == "fast":
        assert parts[b"eXIf"] == exif
    with Image.open(io.BytesIO(data)) as source, Image.open(io.BytesIO(output)) as decoded:
        assert source.convert("RGBA").tobytes() == decoded.convert("RGBA").tobytes()

    source = args.output / "中文 空格.png"
    source.write_bytes(data)
    destination = args.output / "中文 输出.png"
    result = subprocess.run([str(exe), str(source), "-o", str(destination), "--preset", args.preset],
                            capture_output=True, timeout=180, check=True)
    assert destination.read_bytes() == output and source.read_bytes() == data
    assert json.loads(result.stdout)["bytes"] == len(output)
    for bad in (b"not PNG", data[:-1], data[:40] + bytes([data[40] ^ 1]) + data[41:]):
        reject(bad, b"luma:")
    for target in ("nan", "inf", "-1", "101"):
        reject(data, b"--target", "--target", target)

    if args.preset == "quality":
        # Omitted --preset must behave identically to explicit quality.
        result = subprocess.run([str(exe), "-", "-o", "-"], input=data,
                                capture_output=True, timeout=180, check=True)
        assert result.stdout == output
        sample = (ROOT / "bench/images/photos/kodim03.png").read_bytes()
        _, report = verify("target-70", sample, "--target", "70")
        assert report["target"] == 70 and not report["miss"] and report["score"] >= 69.999

        # More than 256 colors cannot be represented exactly in a palette PNG.
        with Image.open(io.BytesIO(sample)) as photo:
            photo = photo.convert("RGB")
            photo.thumbnail((96, 80))
            assert photo.getcolors(256) is None
            buffer = io.BytesIO()
            photo.save(buffer, format="PNG", compress_level=0)
        _, report = verify("unreachable-target", buffer.getvalue(), "--target", "100")
        assert report["target"] == 100 and report["miss"]
        assert report["outcome"] == "miss" and report["score"] < 100
    else:
        reject(data, b"--target", "--target", "90")
        icon = (ROOT / "bench/images/icons/twemoji-1f4a1.png").read_bytes()
        reject(icon, b"no_smaller_output")

    # Exercise the selected preset's failure contract, including file preservation.
    tiny = (SIGNATURE + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 1, 0, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"\0\0", 9)) + chunk(b"IEND", b""))
    reject(tiny, b"no_smaller_output")
    destination = args.output / "refused.png"
    assert not destination.exists()
    result = subprocess.run([str(exe), "-", "-o", str(destination), "--preset", args.preset],
                            input=tiny, capture_output=True, timeout=60)
    assert result.returncode != 0 and not destination.exists()
    source = args.output / "existing.png"
    source.write_bytes(tiny)
    result = subprocess.run([str(exe), str(source), "-o", str(source), "--preset", args.preset],
                            capture_output=True, timeout=60)
    assert result.returncode != 0 and source.read_bytes() == tiny

    large = solid_png()
    animation = animated_png()
    if args.preset == "quality":
        verify("over-8mp", large)
        output, report = verify("animation", animation, "--target", "100")
        with Image.open(io.BytesIO(animation)) as original, Image.open(io.BytesIO(output)) as decoded:
            assert original.n_frames == decoded.n_frames == report["frames"] == 2
            assert original.info["loop"] == decoded.info["loop"] == 2
            for frame in range(2):
                original.seek(frame)
                decoded.seek(frame)
                assert original.info["duration"] == decoded.info["duration"]
                assert original.convert("RGBA").tobytes() == decoded.convert("RGBA").tobytes()
    else:
        reject(large, b"pixel limit")
        reject(animation, b"animation")

    summary = {"exe_sha256": hashlib.sha256(exe.read_bytes()).hexdigest(), "preset": args.preset,
               "rows": rows, "checks_passed": True}
    (args.output / "results.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
