//! Native PNG/JPEG compression CLI with quality and fast presets.

use std::{
    env, fs,
    io::{self, Read, Write},
    process,
    sync::OnceLock,
    time::Instant,
};

const USAGE: &str = "usage: luma IN.png -o OUT.png|OUT.webp [--preset quality|fast] [--format png|webp] [--target auto|T]
       luma IN.jpg -o OUT.webp [--preset quality|fast]
       luma - -o - --preset fast --format webp    (binary stdin/stdout, report on stderr)";

fn main() {
    if let Err(e) = run() {
        eprintln!("luma: {e}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut settings = luma::Settings { clock: Some(now_ms), ..Default::default() };
    let mut preset = String::from("quality");
    let mut target_set = false;
    let (mut input, mut output, mut format) = (None, None, None);
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
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
            "--target" => {
                target_set = true;
                let v = value()?;
                settings.target = if v == "auto" { None } else { Some(v.parse().map_err(|_| format!("bad --target: {v}"))?) };
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ if (arg.starts_with('-') && arg != "-") || input.is_some() => return Err(format!("unexpected argument: {arg}\n{USAGE}").into()),
            _ => input = Some(arg),
        }
    }
    if preset == "fast" && target_set {
        return Err("--target is only supported by the quality preset".into());
    }
    if settings.target.is_some_and(|target| !target.is_finite() || !(0.0..=100.0).contains(&target)) {
        return Err("--target must be auto or a finite number from 0 to 100".into());
    }
    if format.as_deref().is_some_and(|f| !matches!(f, "png" | "webp")) {
        return Err("--format must be png or webp; JPEG output is disabled".into());
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
        let result = luma::compress_fast(&input)?;
        write_output(&output, &result.png)?;
        report(&output, &result.json());
        return Ok(());
    }

    let output = output.ok_or(USAGE)?;
    if is_jpeg(&input) {
        return match format.as_deref() {
            Some("webp") => write_webp(&input, &output, &settings),
            None if output.to_lowercase().ends_with(".webp") => write_webp(&input, &output, &settings),
            None => Err("JPEG output is disabled; use --format webp to convert JPEG input".into()),
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
    let result = luma::compress(&input, &settings)?;
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
    let result = luma::compress_webp_fast(input)?;
    write_output(output, &result.webp)?;
    report(output, &result.json());
    Ok(())
}

#[cfg(not(feature = "webp"))]
fn write_webp_fast(_: &[u8], _: &str) -> Result<(), Box<dyn std::error::Error>> {
    Err("this luma was built without the webp feature".into())
}

#[cfg(feature = "webp")]
fn write_webp(input: &[u8], output: &str, settings: &luma::Settings) -> Result<(), Box<dyn std::error::Error>> {
    let result = luma::compress_webp(input, settings)?;
    write_output(output, &result.webp)?;
    report(output, &result.json());
    Ok(())
}

#[cfg(not(feature = "webp"))]
fn write_webp(_: &[u8], _: &str, _: &luma::Settings) -> Result<(), Box<dyn std::error::Error>> {
    Err("this luma was built without the webp feature".into())
}

#[cfg(feature = "jpeg-decode")]
fn is_jpeg(input: &[u8]) -> bool {
    luma::jpeg::is_jpeg(input)
}

#[cfg(not(feature = "jpeg-decode"))]
fn is_jpeg(_: &[u8]) -> bool {
    false
}

fn now_ms() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}
