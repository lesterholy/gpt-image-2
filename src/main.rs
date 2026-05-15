use clap::{Args, Parser, Subcommand};
use gpt_image_2::{
    env_or_default, optional_env, run_edit, run_generate, run_generate_batch, BatchOptions,
    EditOptions, GenerateOptions, OutputOptions, PromptFields, DEFAULT_CONCURRENCY,
    DEFAULT_DOWNSCALE_SUFFIX, DEFAULT_MODEL, DEFAULT_N, DEFAULT_OUTPUT_FORMAT, DEFAULT_OUTPUT_PATH,
    DEFAULT_QUALITY, DEFAULT_SIZE,
};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "gpt-image-2",
    version,
    about = "Fallback CLI for explicit image generation or editing via GPT Image models."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new image.
    Generate(Box<GenerateArgs>),
    /// Edit one or more existing images.
    Edit(Box<EditArgs>),
    /// Generate multiple prompts from a JSONL file.
    GenerateBatch(Box<BatchArgs>),
}

#[derive(Debug, Args)]
struct SharedArgs {
    /// GPT Image model.
    #[arg(long, default_value_t = default_model())]
    model: String,

    /// Text prompt. Use --prompt or --prompt-file.
    #[arg(long)]
    prompt: Option<String>,

    /// Path to a UTF-8 prompt file. Use --prompt or --prompt-file.
    #[arg(long)]
    prompt_file: Option<String>,

    /// Number of images to request. Imagegen CLI allows 1 through 10.
    #[arg(long, value_parser = n_value, default_value_t = default_n())]
    n: u8,

    /// Image size. Defaults to auto.
    #[arg(long, default_value_t = default_size())]
    size: String,

    /// Quality: low, medium, high, or auto.
    #[arg(long, value_parser = quality_value, default_value_t = default_quality())]
    quality: String,

    /// Background: transparent, opaque, or auto.
    #[arg(long, value_parser = background_value)]
    background: Option<String>,

    /// Output format: png, jpeg, jpg, or webp.
    #[arg(long = "output-format", value_parser = output_format_value, default_value_t = default_output_format())]
    output_format: String,

    /// Output compression from 0 through 100, for jpeg/webp.
    #[arg(long = "output-compression", value_parser = compression_value)]
    output_compression: Option<u8>,

    /// Moderation: auto or low.
    #[arg(long, value_parser = moderation_value)]
    moderation: Option<String>,

    /// Output path. Defaults to output/imagegen/output.png.
    #[arg(long = "out", default_value_t = env_or_default("IMAGE_OUTPUT", DEFAULT_OUTPUT_PATH))]
    out: String,

    /// Output directory. One-off naming becomes image_1.<ext>, image_2.<ext>, ...
    #[arg(long = "out-dir")]
    out_dir: Option<String>,

    /// Overwrite existing output files.
    #[arg(long)]
    force: bool,

    /// Print computed request and output path(s), without calling the API.
    #[arg(long)]
    dry_run: bool,

    /// Enable prompt augmentation fields.
    #[arg(long = "augment", default_value_t = true, action = clap::ArgAction::SetTrue)]
    augment: bool,

    /// Disable prompt augmentation fields.
    #[arg(long = "no-augment", action = clap::ArgAction::SetTrue, overrides_with = "augment")]
    no_augment: bool,

    /// Use-case slug for prompt augmentation.
    #[arg(long = "use-case")]
    use_case: Option<String>,

    /// Scene/background prompt augmentation hint.
    #[arg(long)]
    scene: Option<String>,

    /// Subject prompt augmentation hint.
    #[arg(long)]
    subject: Option<String>,

    /// Style/medium prompt augmentation hint.
    #[arg(long)]
    style: Option<String>,

    /// Composition/framing prompt augmentation hint.
    #[arg(long)]
    composition: Option<String>,

    /// Lighting/mood prompt augmentation hint.
    #[arg(long)]
    lighting: Option<String>,

    /// Color palette prompt augmentation hint.
    #[arg(long)]
    palette: Option<String>,

    /// Materials/textures prompt augmentation hint.
    #[arg(long)]
    materials: Option<String>,

    /// Verbatim in-image text prompt augmentation hint.
    #[arg(long)]
    text: Option<String>,

    /// Constraints prompt augmentation hint.
    #[arg(long)]
    constraints: Option<String>,

    /// Negative constraints prompt augmentation hint.
    #[arg(long)]
    negative: Option<String>,

    /// Generate an additional downscaled copy with max dimension.
    #[arg(long = "downscale-max-dim", value_parser = downscale_dim_value)]
    downscale_max_dim: Option<u32>,

    /// Downscaled copy suffix.
    #[arg(long = "downscale-suffix", default_value_t = default_downscale_suffix())]
    downscale_suffix: String,
}

#[derive(Debug, Args)]
struct GenerateArgs {
    #[command(flatten)]
    shared: SharedArgs,
}

#[derive(Debug, Args)]
struct EditArgs {
    #[command(flatten)]
    shared: SharedArgs,

    /// Input image path. Repeat for multi-image edits.
    #[arg(long = "image", required = true)]
    images: Vec<String>,

    /// Optional mask image.
    #[arg(long)]
    mask: Option<String>,

    /// Input fidelity: low or high. Not supported for gpt-image-2.
    #[arg(long = "input-fidelity", value_parser = input_fidelity_value)]
    input_fidelity: Option<String>,
}

#[derive(Debug, Args)]
struct BatchArgs {
    #[command(flatten)]
    shared: SharedArgs,

    /// Path to JSONL file. One job per line.
    #[arg(long)]
    input: String,

    /// Number of concurrent jobs.
    #[arg(long, value_parser = concurrency_value, default_value_t = DEFAULT_CONCURRENCY)]
    concurrency: u8,

    /// Max attempts per job.
    #[arg(long = "max-attempts", value_parser = attempts_value, default_value_t = 3)]
    max_attempts: u8,

    /// Stop on first failed job.
    #[arg(long = "fail-fast")]
    fail_fast: bool,
}

fn default_model() -> String {
    env_or_default("IMAGE_MODEL", DEFAULT_MODEL)
}

fn default_size() -> String {
    env_or_default("IMAGE_SIZE", DEFAULT_SIZE)
}

fn default_quality() -> String {
    env_or_default("IMAGE_QUALITY", DEFAULT_QUALITY)
}

fn default_output_format() -> String {
    env_or_default("IMAGE_OUTPUT_FORMAT", DEFAULT_OUTPUT_FORMAT)
}

fn default_downscale_suffix() -> String {
    env_or_default("IMAGE_DOWNSCALE_SUFFIX", DEFAULT_DOWNSCALE_SUFFIX)
}

fn default_n() -> u8 {
    optional_env("IMAGE_N")
        .and_then(|value| n_value(&value).ok())
        .unwrap_or(DEFAULT_N)
}

fn n_value(value: &str) -> Result<u8, String> {
    bounded_u8(value, 1, 10, "value must be from 1 to 10")
}

fn concurrency_value(value: &str) -> Result<u8, String> {
    bounded_u8(value, 1, 25, "value must be from 1 to 25")
}

fn attempts_value(value: &str) -> Result<u8, String> {
    bounded_u8(value, 1, 10, "value must be from 1 to 10")
}

fn downscale_dim_value(value: &str) -> Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| "value must be at least 1".to_string())?;
    if parsed < 1 {
        return Err("value must be at least 1".to_string());
    }
    Ok(parsed)
}

fn bounded_u8(value: &str, min: u8, max: u8, message: &str) -> Result<u8, String> {
    let parsed = value.parse::<u8>().map_err(|_| message.to_string())?;
    if !(min..=max).contains(&parsed) {
        return Err(message.to_string());
    }
    Ok(parsed)
}

fn compression_value(value: &str) -> Result<u8, String> {
    bounded_u8(value, 0, 100, "value must be from 0 to 100")
}

fn quality_value(value: &str) -> Result<String, String> {
    match value {
        "low" | "medium" | "high" | "auto" => Ok(value.to_string()),
        _ => Err("value must be one of low, medium, high, auto".to_string()),
    }
}

fn background_value(value: &str) -> Result<String, String> {
    match value {
        "transparent" | "opaque" | "auto" => Ok(value.to_string()),
        _ => Err("value must be one of transparent, opaque, auto".to_string()),
    }
}

fn output_format_value(value: &str) -> Result<String, String> {
    let value = value.to_ascii_lowercase();
    match value.as_str() {
        "png" | "jpeg" | "jpg" | "webp" => Ok(if value == "jpg" {
            "jpeg".to_string()
        } else {
            value
        }),
        _ => Err("value must be one of png, jpeg, jpg, webp".to_string()),
    }
}

fn moderation_value(value: &str) -> Result<String, String> {
    match value {
        "auto" | "low" => Ok(value.to_string()),
        _ => Err("value must be one of auto, low".to_string()),
    }
}

fn input_fidelity_value(value: &str) -> Result<String, String> {
    match value {
        "low" | "high" => Ok(value.to_string()),
        _ => Err("value must be one of low, high".to_string()),
    }
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Generate(args) => run_generate_command(*args),
        Command::Edit(args) => run_edit_command(*args),
        Command::GenerateBatch(args) => run_batch_command(*args),
    };

    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("gpt-image-2: error: {error}");
            std::process::exit(1);
        }
    }
}

fn run_generate_command(args: GenerateArgs) -> gpt_image_2::Result<i32> {
    let report = run_generate(generate_options(args.shared)?)?;
    print_json(&report)?;
    Ok(0)
}

fn run_edit_command(args: EditArgs) -> gpt_image_2::Result<i32> {
    let mut options = generate_options(args.shared)?;
    options.images = args.images;
    let report = run_edit(EditOptions {
        generate: options,
        mask: args.mask.map(PathBuf::from),
        input_fidelity: args.input_fidelity,
    })?;
    print_json(&report)?;
    Ok(0)
}

fn run_batch_command(args: BatchArgs) -> gpt_image_2::Result<i32> {
    let out_dir = args
        .shared
        .out_dir
        .clone()
        .ok_or_else(|| gpt_image_2::Error::message("generate-batch requires --out-dir"))?;
    let report = run_generate_batch(BatchOptions {
        base: generate_options(args.shared)?,
        input: PathBuf::from(args.input),
        out_dir: PathBuf::from(out_dir),
        concurrency: args.concurrency,
        max_attempts: args.max_attempts,
        fail_fast: args.fail_fast,
    })?;
    let exit_code = report.exit_code;
    print_json(&report)?;
    Ok(exit_code)
}

fn generate_options(args: SharedArgs) -> gpt_image_2::Result<GenerateOptions> {
    let augment = if args.no_augment { false } else { args.augment };
    Ok(GenerateOptions {
        prompt: args.prompt,
        prompt_file: args.prompt_file.map(PathBuf::from),
        images: Vec::new(),
        output: OutputOptions {
            out: PathBuf::from(args.out),
            out_dir: args.out_dir.map(PathBuf::from),
            output_format: args.output_format,
            force: args.force,
            downscale_max_dim: args.downscale_max_dim,
            downscale_suffix: args.downscale_suffix,
        },
        model: args.model,
        n: args.n,
        size: args.size,
        quality: args.quality,
        background: args.background,
        output_compression: args.output_compression,
        moderation: args.moderation,
        augment,
        fields: PromptFields {
            use_case: args.use_case,
            scene: args.scene,
            subject: args.subject,
            style: args.style,
            composition: args.composition,
            lighting: args.lighting,
            palette: args.palette,
            materials: args.materials,
            text: args.text,
            constraints: args.constraints,
            negative: args.negative,
        },
        dry_run: args.dry_run,
    })
}

fn print_json<T: Serialize>(value: &T) -> gpt_image_2::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn generate_args_accept_imagegen_controls() {
        let cli = Cli::try_parse_from([
            "gpt-image-2",
            "generate",
            "--prompt",
            "cat",
            "--quality",
            "high",
            "--size",
            "2048x1152",
            "--output-format",
            "webp",
            "--dry-run",
        ])
        .unwrap();

        match cli.command {
            Command::Generate(args) => {
                assert_eq!(args.shared.quality, "high");
                assert_eq!(args.shared.size, "2048x1152");
                assert_eq!(args.shared.output_format, "webp");
                assert!(args.shared.dry_run);
            }
            _ => panic!("expected generate command"),
        }
    }

    #[test]
    fn edit_args_require_image() {
        let result = Cli::try_parse_from(["gpt-image-2", "edit", "--prompt", "cat"]);
        assert!(result.is_err());
    }

    #[test]
    fn batch_args_require_out_dir() {
        let cli = Cli::try_parse_from([
            "gpt-image-2",
            "generate-batch",
            "--input",
            "jobs.jsonl",
            "--prompt",
            "base",
        ])
        .unwrap();

        let Command::GenerateBatch(args) = cli.command else {
            panic!("expected generate-batch command");
        };
        let error = run_batch_command(*args).unwrap_err();
        assert!(error
            .to_string()
            .contains("generate-batch requires --out-dir"));
    }

    #[test]
    fn no_augment_disables_default_augmentation() {
        let cli = Cli::try_parse_from([
            "gpt-image-2",
            "generate",
            "--prompt",
            "cat",
            "--no-augment",
            "--dry-run",
        ])
        .unwrap();

        let Command::Generate(args) = cli.command else {
            panic!("expected generate command");
        };
        assert!(args.shared.no_augment);
    }
}
