"""Check a downloaded Windows WebP CLI; requires Pillow, never builds Rust."""

import argparse
import hashlib
import io
import json
import subprocess
import time
import zlib
from pathlib import Path

from PIL import Image, ImageChops, ImageOps, ImageStat
from verify_fast import chunk

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--preset", choices=["quality", "fast"], default="quality")
    args = parser.parse_args()
    exe = args.exe.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    command = [str(exe), "-", "-o", "-", "--format", "webp", "--preset", args.preset]
    rows = []

    def verify(name, data):
        started = time.perf_counter()
        result = subprocess.run(command, input=data, capture_output=True, timeout=180, check=True)
        seconds = time.perf_counter() - started
        report = json.loads(result.stderr)
        assert report["format"] == "webp"
        if args.preset == "fast":
            assert report["preset"] == "fast" and report["score"] is None
            assert report["quality"] == 80 and report["method"] == 3
        assert report["bytes"] == len(result.stdout)
        assert len(result.stdout) < len(data), "successful output must be smaller"
        assert result.stdout[:4] == b"RIFF" and result.stdout[8:12] == b"WEBP"
        assert int.from_bytes(result.stdout[4:8], "little") == len(result.stdout) - 8
        with Image.open(io.BytesIO(data)) as source:
            expected = ImageOps.exif_transpose(source).convert("RGBA")
        with Image.open(io.BytesIO(result.stdout)) as encoded:
            assert encoded.format == "WEBP" and encoded.size == expected.size
            output = encoded.convert("RGBA")
        if expected.getchannel("A").getextrema()[0] < 255:
            if args.preset == "fast":
                assert output.getchannel("A").tobytes() == expected.getchannel("A").tobytes(), "alpha changed"
            else:
                assert output.getchannel("A").getextrema()[0] < 255, "transparency lost"
        destination = args.output / (name + ".webp")
        destination.write_bytes(result.stdout)
        rows.append({"name": name, "input_bytes": len(data), "output_bytes": len(result.stdout),
                     "seconds": round(seconds, 3), "report": report})
        print(json.dumps(rows[-1]), flush=True)
        return expected, output

    for relative in ["illustrations/bicycle-diagram-es.png", "photos/kodim03.png", "photos-alpha/cup-of-tea.png"]:
        source = ROOT / "bench/images" / relative
        data = source.read_bytes()
        verify(source.stem, data)
        assert source.read_bytes() == data

    # JPEG fixtures come from a bundled photo, including rotation and grayscale decoding.
    with Image.open(ROOT / "bench/images/photos/kodim03.png") as photo:
        photo = photo.convert("RGB")
        photo.thumbnail((320, 240))
    for orientation, mode in [(1, "RGB"), (6, "RGB"), (1, "L")]:
        jpeg = io.BytesIO()
        exif = Image.Exif()
        exif[274] = orientation
        photo.convert(mode).save(jpeg, format="JPEG", quality=90, exif=exif)
        data = jpeg.getvalue()
        expected, output = verify(f"jpeg-{mode}-orientation-{orientation}", data)
        if orientation == 6:
            # Correct dimensions alone would not catch a rotation in the wrong direction.
            error = lambda image: sum(ImageStat.Stat(ImageChops.difference(image.convert("RGB"), output.convert("RGB"))).mean)
            assert error(expected) < error(expected.transpose(Image.Transpose.ROTATE_180))
        rejected = subprocess.run([str(exe), "-", "-o", "-", "--format", "jpeg"],
                                  input=data, capture_output=True, timeout=10)
        assert rejected.returncode != 0 and not rejected.stdout
        assert b"JPEG output is disabled" in rejected.stderr

    # Extension-selected output, Chinese/spaced paths, and binary output decoding.
    source = args.output / "中文 输入.jpg"
    source.write_bytes(data)
    destination = args.output / "中文 输出.webp"
    result = subprocess.run([str(exe), str(source), "-o", str(destination), "--preset", args.preset],
                            capture_output=True, timeout=180, check=True)
    assert json.loads(result.stdout)["format"] == "webp" and source.read_bytes() == data
    with Image.open(destination) as image:
        image.load()
        assert image.format == "WEBP" and image.size == photo.size
    for bad in (b"not an image", b"\xff\xd8\xffbroken JPEG"):
        result = subprocess.run(command, input=bad, capture_output=True, timeout=10)
        assert result.returncode != 0 and not result.stdout and result.stderr

    # PNG stores ICC compressed; WebP stores it uncompressed. Include metadata in the
    # final size check even when the encoded pixels themselves would be smaller.
    buffer = io.BytesIO()
    Image.new("RGB", (32, 24), "red").save(buffer, format="PNG")
    small = buffer.getvalue()
    profile = b"test profile " * 1024
    icc = chunk(b"iCCP", b"profile\0\0" + zlib.compress(profile))
    tagged = small[:33] + icc + small[33:]
    result = subprocess.run(command, input=tagged, capture_output=True, timeout=60)
    assert result.returncode != 0 and not result.stdout and b"no_smaller_output" in result.stderr
    destination = args.output / "refused.webp"
    assert not destination.exists()
    result = subprocess.run([str(exe), "-", "-o", str(destination), "--preset", args.preset],
                            input=tagged, capture_output=True, timeout=60)
    assert result.returncode != 0 and not destination.exists()
    destination = args.output / "existing.webp"
    sentinel = (args.output / "kodim03.webp").read_bytes()
    destination.write_bytes(sentinel)
    result = subprocess.run([str(exe), "-", "-o", str(destination), "--preset", args.preset],
                            input=tagged, capture_output=True, timeout=60)
    assert result.returncode != 0 and destination.read_bytes() == sentinel

    if args.preset == "fast":
        from PIL.PngImagePlugin import PngInfo

        # Reuse a real photo: all EXIF transforms, stripped comments, no EXIF in WebP.
        for orientation in range(1, 9):
            exif = Image.Exif()
            exif[274] = orientation
            metadata = PngInfo()
            metadata.add_text("Comment", "must be stripped")
            buffer = io.BytesIO()
            photo.save(buffer, format="PNG", exif=exif, pnginfo=metadata)
            data = buffer.getvalue()
            expected, output = verify(f"png-orientation-{orientation}", data)
            error = lambda image: sum(ImageStat.Stat(ImageChops.difference(image.convert("RGB"), output.convert("RGB"))).mean)
            assert error(expected) < error(expected.transpose(Image.Transpose.ROTATE_180))
            with Image.open(args.output / f"png-orientation-{orientation}.webp") as encoded:
                assert not encoded.getexif() and "Comment" not in encoded.info

        # Reject animation, invalid CRC, malformed EXIF and oversized headers before output.
        large = bytearray(data[16:29])
        large[:8] = (4000).to_bytes(4, "big") * 2
        animated = data[:33] + chunk(b"acTL", (2).to_bytes(4, "big") + bytes(4)) + data[33:]
        broken_exif = data[:33] + chunk(b"eXIf", b"broken") + data[33:]
        oversized = data[:8] + chunk(b"IHDR", large) + data[33:]
        corrupt = data[:40] + bytes([data[40] ^ 1]) + data[41:]
        for bad in (animated, broken_exif, oversized, corrupt):
            result = subprocess.run(command, input=bad, capture_output=True, timeout=10)
            assert result.returncode != 0 and not result.stdout and result.stderr
        for option in (["--target", "80"], ["--webp-method", "6"], ["--format", "jpeg"]):
            result = subprocess.run(command + option, input=data, capture_output=True, timeout=10)
            assert result.returncode != 0 and not result.stdout

        # JPEG pixel limit is checked after headers, before allocating decoded pixels.
        jpeg = bytearray(source.read_bytes())
        offset = 2
        while offset < len(jpeg):
            assert jpeg[offset] == 0xff
            marker = jpeg[offset + 1]
            length = int.from_bytes(jpeg[offset + 2:offset + 4], "big")
            if marker in (0xc0, 0xc1, 0xc2):
                jpeg[offset + 5:offset + 9] = (4000).to_bytes(2, "big") * 2
                break
            offset += length + 2
        else:
            raise AssertionError("JPEG fixture has no frame header")
        result = subprocess.run(command, input=jpeg, capture_output=True, timeout=10)
        assert result.returncode != 0 and not result.stdout and b"pixel limit" in result.stderr

    summary = {"exe_sha256": hashlib.sha256(exe.read_bytes()).hexdigest(), "preset": args.preset,
               "rows": rows, "checks_passed": True}
    (args.output / "results.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
