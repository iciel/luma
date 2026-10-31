"""Check a downloaded Windows WebP CLI; requires Pillow, never builds Rust."""

import argparse
import hashlib
import io
import json
import subprocess
import time
from pathlib import Path

from PIL import Image, ImageChops, ImageOps, ImageStat

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    exe = args.exe.resolve()
    args.output.mkdir(parents=True, exist_ok=True)
    command = [str(exe), "-", "-o", "-", "--format", "webp"]
    rows = []

    def verify(name, data):
        started = time.perf_counter()
        result = subprocess.run(command, input=data, capture_output=True, timeout=180, check=True)
        seconds = time.perf_counter() - started
        report = json.loads(result.stderr)
        assert report["format"] == "webp"
        assert result.stdout[:4] == b"RIFF" and result.stdout[8:12] == b"WEBP"
        assert int.from_bytes(result.stdout[4:8], "little") == len(result.stdout) - 8
        with Image.open(io.BytesIO(data)) as source:
            expected = ImageOps.exif_transpose(source).convert("RGBA")
        with Image.open(io.BytesIO(result.stdout)) as encoded:
            assert encoded.format == "WEBP" and encoded.size == expected.size
            output = encoded.convert("RGBA")
        if expected.getchannel("A").getextrema()[0] < 255:
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
    result = subprocess.run([str(exe), str(source), "-o", str(destination)],
                            capture_output=True, timeout=180, check=True)
    assert json.loads(result.stdout)["format"] == "webp" and source.read_bytes() == data
    with Image.open(destination) as image:
        image.load()
        assert image.format == "WEBP" and image.size == photo.size
    for bad in (b"not an image", b"\xff\xd8\xffbroken JPEG"):
        result = subprocess.run(command, input=bad, capture_output=True, timeout=10)
        assert result.returncode != 0 and not result.stdout and result.stderr
    summary = {"exe_sha256": hashlib.sha256(exe.read_bytes()).hexdigest(), "rows": rows, "checks_passed": True}
    (args.output / "results.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
