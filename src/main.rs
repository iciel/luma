//! Native PNG/JPEG compression CLI with quality and fast presets.

use std::{
    env, fs,
    io::{self, Read, Write},
    process,
    sync::OnceLock,
    time::Instant,
};

use pngdeer::{codec, search};

const USAGE: &str = "usage: pngdeer IN.png -o OUT.png [--preset quality|fast] [--format png|webp] [--target auto|T] [--preview-pixels N] [--variants SPEC]
       pngdeer IN.jpg -o OUT.webp [--target auto|T]
       pngdeer IN.png --score OTHER
       pngdeer IN.png -o OUT.png --upscale K
       pngdeer - -o - --preset fast [--format webp]    (binary stdin/stdout, report on stderr)";

fn main() {
    if let Err(e) = run() {
        eprintln!("pngdeer: {e}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = pngdeer::Settings { clock: Some(now_ms), ..Default::default() };
    let mut preset = String::from("quality");
    let mut search_options = false;
    let (mut input, mut output, mut score_against, mut upscale, mut format) = (None, None, None, None::<usize>, None);
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        search_options |= arg.starts_with("--webp-");
        let mut value = || args.next().ok_or_else(|| format!("{arg} needs a value\n{USAGE}"));
        match arg.as_str() {
            "-o" => output = Some(value()?),
            "--preset" => {
                preset = value()?;
                if !matches!(preset.as_str(), "quality" | "fast") {
                    return Err("--preset must be quality or fast".into());
                }
            }
            "--format" => format = Some(value()?),
            "--webp-method" => settings.webp.method = value()?.parse()?,
            "--webp-lossless-level" => settings.webp.slow_lossless_level = Some(value()?.parse()?),
            "--webp-sharp-yuv" => settings.webp.sharp_yuv = value()? != "0",
            "--webp-alpha-filter" => settings.webp.alpha_filtering = value()?.parse()?,
            "--webp-autofilter" => settings.webp.autofilter = value()? != "0",
            "--webp-large-alpha-effort" => settings.webp.large_alpha_effort = value()?.parse()?,
            "--webp-reference" => {
                let v = value()?;
                settings.webp.reference_quality = if v == "none" { None } else { Some(v.parse()?) };
            }
            "--target" => {
                search_options = true;
                let v = value()?;
                settings.target = if v == "auto" { None } else { Some(v.parse().map_err(|_| format!("bad --target: {v}"))?) };
            }
            "--preview-pixels" => {
                search_options = true;
                settings.preview_pixels = value()?.parse()?;
            }
            "--variants" => {
                search_options = true;
                settings.variants = search::parse_variants(&value()?)?;
            }
            "--score" => score_against = Some(value()?),
            "--upscale" => upscale = Some(value()?.parse()?),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ if (arg.starts_with('-') && arg != "-") || input.is_some() => return Err(format!("unexpected argument: {arg}\n{USAGE}").into()),
            _ => input = Some(arg),
        }
    }
    if preset == "fast" && (search_options || score_against.is_some() || upscale.is_some() || format.as_deref().is_some_and(|f| !matches!(f, "png" | "webp"))) {
        return Err("fast preset accepts PNG or WebP output and does not use search, score, upscale or WebP tuning options".into());
    }
    let input_path = input.ok_or(USAGE)?;
    let input = if input_path == "-" {
        let mut bytes = Vec::new();
        io::stdin().lock().take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err("stdin exceeds the 64 MiB limit".into());
        }
        bytes
    } else {
        fs::read(input_path)?
    };
    if preset == "fast" {
        let output = output.ok_or(USAGE)?;
        if format.as_deref() == Some("webp") || (format.is_none() && output.to_lowercase().ends_with(".webp")) {
            return write_webp_fast(&input, &output);
        }
        let result = pngdeer::compress_fast(&input)?;
        write_output(&output, &result.png)?;
        report(&output, &result.json());
        return Ok(());
    }

    if let Some(other) = score_against {
        if pngdeer::apng::is_animated(&input) {
            println!("{:.4}", frame_scores(&input, &fs::read(other)?)?.into_iter().fold(f64::INFINITY, f64::min));
            return Ok(());
        }
        let (a, b) = (decode_any(&input)?, decode_any(&fs::read(other)?)?);
        if (a.width, a.height) != (b.width, b.height) {
            return Err("images differ in size".into());
        }
        println!("{:.4}", search::score(&a.pixels, &b.pixels, a.width, a.height)?);
        return Ok(());
    }
    let output = output.ok_or(USAGE)?;
    if let Some(k) = upscale {
        let image = codec::decode(&input)?;
        let (w, h, pixels) = (image.width, image.height, &image.pixels);
        let big: Vec<_> = (0..h * k).flat_map(|y| (0..w * k).map(move |x| pixels[(y / k) * w + x / k])).collect();
        write_output(&output, &codec::encode_rgba(&big, w * k, h * k))?;
        return Ok(());
    }
    if is_jpeg(&input) {
        return match format.as_deref() {
            Some("webp") => write_webp(&input, &output, &settings),
            None if output.to_lowercase().ends_with(".webp") => write_webp(&input, &output, &settings),
            Some("jpeg") | None => Err("JPEG output is disabled; use --format webp to convert JPEG input".into()),
            Some(other) => Err(format!("a JPEG converts to webp, not {other}").into()),
        };
    }
    let webp = match format.as_deref() {
        Some("webp") => true,
        Some("png") => false,
        Some(other) => return Err(format!("unknown --format: {other} (png or webp)").into()),
        None => output.to_lowercase().ends_with(".webp"),
    };
    if webp {
        return write_webp(&input, &output, &settings);
    }
    let result = pngdeer::compress(&input, &settings)?;
    write_output(&output, &result.png)?;
    report(&output, &result.json());
    Ok(())
}

fn write_output(path: &str, bytes: &[u8]) -> io::Result<()> {
    if path == "-" {
        let mut stdout = io::stdout().lock();
        stdout.write_all(bytes)?;
        stdout.flush()
    } else {
        fs::write(path, bytes)
    }
}

fn report(output: &str, json: &str) {
    if output == "-" {
        eprintln!("{json}");
    } else {
        println!("{json}");
    }
}

#[cfg(feature = "webp")]
fn write_webp_fast(input: &[u8], output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let result = pngdeer::compress_webp_fast(input)?;
    write_output(output, &result.webp)?;
    report(output, &result.json());
    Ok(())
}

#[cfg(not(feature = "webp"))]
fn write_webp_fast(_: &[u8], _: &str) -> Result<(), Box<dyn std::error::Error>> {
    Err("this pngdeer was built without the webp feature".into())
}

#[cfg(feature = "webp")]
fn write_webp(input: &[u8], output: &str, settings: &pngdeer::Settings) -> Result<(), Box<dyn std::error::Error>> {
    let result = pngdeer::compress_webp(input, settings)?;
    write_output(output, &result.webp)?;
    report(output, &result.json());
    Ok(())
}

#[cfg(not(feature = "webp"))]
fn write_webp(_: &[u8], _: &str, _: &pngdeer::Settings) -> Result<(), Box<dyn std::error::Error>> {
    Err("this pngdeer was built without the webp feature".into())
}

#[cfg(feature = "jpeg-decode")]
fn is_jpeg(input: &[u8]) -> bool {
    pngdeer::jpeg::is_jpeg(input)
}

#[cfg(not(feature = "jpeg-decode"))]
fn is_jpeg(_: &[u8]) -> bool {
    false
}

/// SSIMULACRA2 of each frame of animation `a` against the frame animation `b` shows at the same
/// time (b may have merged frames that repeat into longer ones), default images aside.
fn frame_scores(a: &[u8], b: &[u8]) -> Result<Vec<f64>, Box<dyn std::error::Error>> {
    let (a, b) = (pngdeer::apng::decode(a)?, pngdeer::apng::decode(b)?);
    if (a.width, a.height) != (b.width, b.height) {
        return Err("animations differ in size".into());
    }
    let starts = |timing: &pngdeer::apng::Timing| {
        let seconds = |&(num, den): &(u16, u16)| num as f64 / if den == 0 { 100.0 } else { den as f64 };
        timing.delays.iter().scan(0.0, |t, d| Some(std::mem::replace(t, *t + seconds(d)))).collect::<Vec<f64>>()
    };
    let (sa, sb) = (starts(&a.timing), starts(&b.timing));
    fn frame(anim: &pngdeer::apng::Animation, i: usize) -> &[pngdeer::RGBA] {
        let (n, f) = (anim.width * anim.height, i + anim.timing.default_image as usize);
        &anim.pixels[f * n..(f + 1) * n]
    }
    sa.iter()
        .enumerate()
        .map(|(i, &t)| {
            let j = sb.iter().rposition(|&s| s <= t + 1e-9).unwrap_or(0);
            search::score(frame(&a, i), frame(&b, j), a.width, a.height)
        })
        .collect()
}

/// A PNG, a WebP file with the webp feature, or a JPEG with the jpeg-decode feature (turned as its EXIF
/// orientation says, as viewers show it), as RGBA.
fn decode_any(bytes: &[u8]) -> Result<codec::Bitmap, Box<dyn std::error::Error>> {
    #[cfg(feature = "jpeg-decode")]
    if pngdeer::jpeg::is_jpeg(bytes) {
        let photo = pngdeer::jpeg::decode(bytes)?;
        let (pixels, width, height) = pngdeer::jpeg::oriented(photo.pixels, photo.width, photo.height, photo.orientation);
        return Ok(codec::Bitmap { pixels, width, height });
    }
    #[cfg(feature = "webp")]
    if bytes.starts_with(b"RIFF") {
        return pngdeer::webp::decode(bytes);
    }
    codec::decode(bytes)
}

fn now_ms() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}
