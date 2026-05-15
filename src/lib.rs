use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::{DynamicImage, GenericImageView, ImageFormat};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::env;
use std::fmt;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

pub const DEFAULT_MODEL: &str = "gpt-image-2";
pub const DEFAULT_SIZE: &str = "auto";
pub const DEFAULT_QUALITY: &str = "medium";
pub const DEFAULT_OUTPUT_FORMAT: &str = "png";
pub const DEFAULT_N: u8 = 1;
pub const DEFAULT_CONCURRENCY: u8 = 5;
pub const DEFAULT_DOWNSCALE_SUFFIX: &str = "-web";
pub const DEFAULT_OUTPUT_PATH: &str = "output/imagegen/output.png";
pub const DEFAULT_TIMEOUT_SECONDS: u64 = 300;
pub const BASE_URL_ENV_NAME: &str = "GPT_IMAGE_2_BASE_URL";
pub const API_KEY_ENV_NAME: &str = "BASE_URL_API_KEY";

const GPT_IMAGE_MODEL_PREFIX: &str = "gpt-image-";
const GPT_IMAGE_2_MODEL: &str = "gpt-image-2";
const GPT_IMAGE_2_MIN_PIXELS: u64 = 655_360;
const GPT_IMAGE_2_MAX_PIXELS: u64 = 8_294_400;
const GPT_IMAGE_2_MAX_EDGE: u32 = 3840;
const GPT_IMAGE_2_MAX_RATIO: f64 = 3.0;
const MAX_IMAGE_BYTES: u64 = 50 * 1024 * 1024;
const MAX_BATCH_JOBS: usize = 500;
const RETRYABLE_HTTP_STATUSES: &[u16] = &[429, 500, 502, 503, 504, 524];
const API_KEY_PLACEHOLDERS: &[&str] = &["<API_KEY>", "YOUR_API_KEY"];

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Message(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    Http(Box<ureq::Error>),
    Base64(base64::DecodeError),
    Image(image::ImageError),
}

impl Error {
    pub fn message(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => write!(formatter, "{message}"),
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::Http(error) => write!(formatter, "{error}"),
            Self::Base64(error) => write!(formatter, "{error}"),
            Self::Image(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<ureq::Error> for Error {
    fn from(error: ureq::Error) -> Self {
        Self::Http(Box::new(error))
    }
}

impl From<base64::DecodeError> for Error {
    fn from(error: base64::DecodeError) -> Self {
        Self::Base64(error)
    }
}

impl From<image::ImageError> for Error {
    fn from(error: image::ImageError) -> Self {
        Self::Image(error)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_case: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lighting: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub materials: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub negative: Option<String>,
}

#[derive(Clone, Debug)]
pub struct OutputOptions {
    pub out: PathBuf,
    pub out_dir: Option<PathBuf>,
    pub output_format: String,
    pub force: bool,
    pub downscale_max_dim: Option<u32>,
    pub downscale_suffix: String,
}

#[derive(Clone, Debug)]
pub struct GenerateOptions {
    pub prompt: Option<String>,
    pub prompt_file: Option<PathBuf>,
    pub images: Vec<String>,
    pub output: OutputOptions,
    pub model: String,
    pub n: u8,
    pub size: String,
    pub quality: String,
    pub background: Option<String>,
    pub output_compression: Option<u8>,
    pub moderation: Option<String>,
    pub augment: bool,
    pub fields: PromptFields,
    pub dry_run: bool,
}

#[derive(Clone, Debug)]
pub struct EditOptions {
    pub generate: GenerateOptions,
    pub mask: Option<PathBuf>,
    pub input_fidelity: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BatchOptions {
    pub base: GenerateOptions,
    pub input: PathBuf,
    pub out_dir: PathBuf,
    pub concurrency: u8,
    pub max_attempts: u8,
    pub fail_fast: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ImageRunReport {
    pub operation: String,
    pub endpoint: String,
    pub outputs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs_downscaled: Option<Vec<String>>,
    pub dry_run: bool,
    pub payload: Value,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BatchFailure {
    pub job: usize,
    pub error: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct BatchReport {
    pub exit_code: i32,
    pub dry_run: bool,
    pub jobs: Vec<ImageRunReport>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<BatchFailure>,
}

pub fn env_value(names: &[&str]) -> String {
    names
        .iter()
        .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()))
        .unwrap_or_default()
}

pub fn env_or_default(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_string())
}

pub fn optional_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

pub fn image_api_url(base_url: &str, operation: &str) -> Result<String> {
    let base = base_url.trim().trim_end_matches('/');
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err(Error::message(
            "Image API base URL must start with http:// or https://",
        ));
    }
    if operation != "generations" && operation != "edits" {
        return Err(Error::message(format!(
            "Unsupported image operation: {operation}"
        )));
    }
    if base.ends_with(&format!("/v1/images/{operation}")) {
        return Ok(base.to_string());
    }
    if base.ends_with("/v1/images") {
        return Ok(format!("{base}/{operation}"));
    }
    if base.ends_with("/v1") {
        return Ok(format!("{base}/images/{operation}"));
    }
    Ok(format!("{base}/v1/images/{operation}"))
}

fn service_configuration() -> Result<(String, String)> {
    let base_url = env_value(&[BASE_URL_ENV_NAME]);
    let api_key = env_value(&[API_KEY_ENV_NAME]);
    validate_configuration(&base_url, &api_key)?;
    Ok((base_url, api_key))
}

pub fn validate_configuration(base_url: &str, api_key: &str) -> Result<()> {
    if base_url.is_empty() {
        return Err(Error::message("Missing GPT_IMAGE_2_BASE_URL."));
    }
    if api_key.is_empty() || API_KEY_PLACEHOLDERS.contains(&api_key) {
        return Err(Error::message("Missing BASE_URL_API_KEY."));
    }
    image_api_url(base_url, "generations")?;
    Ok(())
}

pub fn validate_model(model: &str) -> Result<()> {
    if !model.starts_with(GPT_IMAGE_MODEL_PREFIX) {
        return Err(Error::message(
            "model must be a GPT Image model (for example gpt-image-2, gpt-image-1.5, gpt-image-1, or gpt-image-1-mini).",
        ));
    }
    Ok(())
}

pub fn validate_size(size: &str, model: &str) -> Result<()> {
    if model == GPT_IMAGE_2_MODEL {
        validate_gpt_image_2_size(size)
    } else if matches!(size, "1024x1024" | "1536x1024" | "1024x1536" | "auto") {
        Ok(())
    } else {
        Err(Error::message(
            "size must be one of 1024x1024, 1536x1024, 1024x1536, or auto for this GPT Image model.",
        ))
    }
}

fn validate_gpt_image_2_size(size: &str) -> Result<()> {
    if size == "auto" {
        return Ok(());
    }
    let (width, height) = parse_size(size).ok_or_else(|| {
        Error::message("size must be auto or WIDTHxHEIGHT, for example 1024x1024.")
    })?;
    let max_edge = width.max(height);
    let min_edge = width.min(height);
    let total_pixels = width as u64 * height as u64;
    if max_edge > GPT_IMAGE_2_MAX_EDGE {
        return Err(Error::message(
            "gpt-image-2 size maximum edge length must be less than or equal to 3840px.",
        ));
    }
    if width % 16 != 0 || height % 16 != 0 {
        return Err(Error::message(
            "gpt-image-2 size width and height must be multiples of 16px.",
        ));
    }
    if max_edge as f64 / min_edge as f64 > GPT_IMAGE_2_MAX_RATIO {
        return Err(Error::message(
            "gpt-image-2 size long edge to short edge ratio must not exceed 3:1.",
        ));
    }
    if !(GPT_IMAGE_2_MIN_PIXELS..=GPT_IMAGE_2_MAX_PIXELS).contains(&total_pixels) {
        return Err(Error::message(
            "gpt-image-2 size total pixels must be at least 655,360 and no more than 8,294,400.",
        ));
    }
    Ok(())
}

fn parse_size(size: &str) -> Option<(u32, u32)> {
    let (width, height) = size.split_once('x')?;
    let width = width.parse::<u32>().ok()?;
    let height = height.parse::<u32>().ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

pub fn validate_quality(quality: &str) -> Result<()> {
    if matches!(quality, "low" | "medium" | "high" | "auto") {
        Ok(())
    } else {
        Err(Error::message(
            "quality must be one of low, medium, high, or auto.",
        ))
    }
}

pub fn validate_background(background: Option<&str>) -> Result<()> {
    if matches!(background, None | Some("transparent" | "opaque" | "auto")) {
        Ok(())
    } else {
        Err(Error::message(
            "background must be one of transparent, opaque, or auto.",
        ))
    }
}

pub fn validate_output_compression(output_compression: Option<u8>) -> Result<()> {
    if let Some(value) = output_compression {
        if value > 100 {
            return Err(Error::message(
                "output_compression must be between 0 and 100",
            ));
        }
    }
    Ok(())
}

pub fn validate_input_fidelity(input_fidelity: Option<&str>) -> Result<()> {
    if matches!(input_fidelity, None | Some("low" | "high")) {
        Ok(())
    } else {
        Err(Error::message("input-fidelity must be one of low or high."))
    }
}

pub fn normalize_output_format(format: &str) -> Result<String> {
    let format = format.to_ascii_lowercase();
    match format.as_str() {
        "" => Ok(DEFAULT_OUTPUT_FORMAT.to_string()),
        "png" | "jpeg" | "webp" => Ok(format),
        "jpg" => Ok("jpeg".to_string()),
        _ => Err(Error::message(
            "output-format must be png, jpeg, jpg, or webp.",
        )),
    }
}

pub fn validate_transparency(background: Option<&str>, output_format: &str) -> Result<()> {
    if background == Some("transparent") && !matches!(output_format, "png" | "webp") {
        Err(Error::message(
            "transparent background requires output-format png or webp.",
        ))
    } else {
        Ok(())
    }
}

pub fn validate_model_specific_options(
    model: &str,
    background: Option<&str>,
    input_fidelity: Option<&str>,
) -> Result<()> {
    if model != GPT_IMAGE_2_MODEL {
        return Ok(());
    }
    if background == Some("transparent") {
        return Err(Error::message(
            "transparent backgrounds are not supported in gpt-image-2, the latest model. Use --model gpt-image-1.5 --background transparent --output-format png instead.",
        ));
    }
    if input_fidelity.is_some() {
        return Err(Error::message(
            "input_fidelity is not supported in gpt-image-2 because image inputs always use high fidelity for this model.",
        ));
    }
    Ok(())
}

pub fn validate_generate_options(options: &GenerateOptions) -> Result<()> {
    validate_model(&options.model)?;
    validate_size(&options.size, &options.model)?;
    validate_quality(&options.quality)?;
    validate_background(options.background.as_deref())?;
    validate_output_compression(options.output_compression)?;
    let output_format = normalize_output_format(&options.output.output_format)?;
    validate_transparency(options.background.as_deref(), &output_format)?;
    validate_model_specific_options(&options.model, options.background.as_deref(), None)?;
    if !(1..=10).contains(&options.n) {
        return Err(Error::message("n must be between 1 and 10"));
    }
    Ok(())
}

pub fn read_prompt(prompt: Option<&str>, prompt_file: Option<&Path>) -> Result<String> {
    if prompt.is_some() && prompt_file.is_some() {
        return Err(Error::message("Use --prompt or --prompt-file, not both."));
    }
    if let Some(path) = prompt_file {
        if !path.exists() {
            return Err(Error::message(format!(
                "Prompt file not found: {}",
                path.display()
            )));
        }
        return Ok(fs::read_to_string(path)?.trim().to_string());
    }
    if let Some(prompt) = prompt {
        let prompt = prompt.trim();
        if !prompt.is_empty() {
            return Ok(prompt.to_string());
        }
    }
    Err(Error::message(
        "Missing prompt. Use --prompt or --prompt-file.",
    ))
}

pub fn augment_prompt(augment: bool, prompt: &str, fields: &PromptFields) -> String {
    if !augment {
        return prompt.to_string();
    }
    let mut sections = Vec::new();
    if let Some(value) = non_empty_option(fields.use_case.as_deref()) {
        sections.push(format!("Use case: {value}"));
    }
    sections.push(format!("Primary request: {prompt}"));
    if let Some(value) = non_empty_option(fields.scene.as_deref()) {
        sections.push(format!("Scene/background: {value}"));
    }
    if let Some(value) = non_empty_option(fields.subject.as_deref()) {
        sections.push(format!("Subject: {value}"));
    }
    if let Some(value) = non_empty_option(fields.style.as_deref()) {
        sections.push(format!("Style/medium: {value}"));
    }
    if let Some(value) = non_empty_option(fields.composition.as_deref()) {
        sections.push(format!("Composition/framing: {value}"));
    }
    if let Some(value) = non_empty_option(fields.lighting.as_deref()) {
        sections.push(format!("Lighting/mood: {value}"));
    }
    if let Some(value) = non_empty_option(fields.palette.as_deref()) {
        sections.push(format!("Color palette: {value}"));
    }
    if let Some(value) = non_empty_option(fields.materials.as_deref()) {
        sections.push(format!("Materials/textures: {value}"));
    }
    if let Some(value) = non_empty_option(fields.text.as_deref()) {
        sections.push(format!("Text (verbatim): \"{value}\""));
    }
    if let Some(value) = non_empty_option(fields.constraints.as_deref()) {
        sections.push(format!("Constraints: {value}"));
    }
    if let Some(value) = non_empty_option(fields.negative.as_deref()) {
        sections.push(format!("Avoid: {value}"));
    }
    sections.join("\n")
}

pub fn build_generate_payload(options: &GenerateOptions, prompt: &str) -> Value {
    let mut payload = Map::new();
    payload.insert("model".to_string(), json!(options.model));
    payload.insert("prompt".to_string(), json!(prompt));
    payload.insert("n".to_string(), json!(options.n));
    payload.insert("size".to_string(), json!(options.size));
    payload.insert("quality".to_string(), json!(options.quality));
    payload.insert(
        "output_format".to_string(),
        json!(normalize_output_format(&options.output.output_format)
            .unwrap_or_else(|_| options.output.output_format.clone())),
    );
    if let Some(value) = non_empty_option(options.background.as_deref()) {
        payload.insert("background".to_string(), json!(value));
    }
    if let Some(value) = options.output_compression {
        payload.insert("output_compression".to_string(), json!(value));
    }
    if let Some(value) = non_empty_option(options.moderation.as_deref()) {
        payload.insert("moderation".to_string(), json!(value));
    }
    Value::Object(payload)
}

pub fn build_edit_payload(options: &EditOptions, prompt: &str) -> Result<Value> {
    validate_input_fidelity(options.input_fidelity.as_deref())?;
    validate_model_specific_options(
        &options.generate.model,
        options.generate.background.as_deref(),
        options.input_fidelity.as_deref(),
    )?;
    let mut payload = build_generate_payload(&options.generate, prompt);
    if let Some(value) = non_empty_option(options.input_fidelity.as_deref()) {
        payload["input_fidelity"] = json!(value);
    }
    Ok(payload)
}

fn non_empty_option(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

fn absolutize(path: &Path) -> Result<PathBuf> {
    let expanded = expand_home(&path.to_string_lossy());
    if expanded.is_absolute() {
        return Ok(expanded);
    }
    Ok(env::current_dir()?.join(expanded))
}

fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(value));
    }
    if let Some(stripped) = value.strip_prefix("~/") {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    PathBuf::from(value)
}

pub fn build_output_paths(output: &OutputOptions, count: u8) -> Result<Vec<PathBuf>> {
    let output_format = normalize_output_format(&output.output_format)?;
    let extension = format!(".{output_format}");
    if let Some(out_dir) = &output.out_dir {
        let base = absolutize(out_dir)?;
        return Ok((1..=count)
            .map(|index| base.join(format!("image_{index}{extension}")))
            .collect());
    }

    let mut out_path = absolutize(&output.out)?;
    if out_path.exists() && out_path.is_dir() {
        return Ok((1..=count)
            .map(|index| out_path.join(format!("image_{index}{extension}")))
            .collect());
    }
    if out_path.extension().is_none() {
        out_path = out_path.with_extension(output_format);
    } else if out_path.extension().and_then(|value| value.to_str()) != Some(output_format.as_str())
    {
        eprintln!(
            "Warning: Output extension {} does not match output-format {}.",
            out_path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default(),
            output_format
        );
    }

    if count == 1 {
        return Ok(vec![out_path]);
    }
    Ok((1..=count)
        .map(|index| {
            out_path.with_file_name(format!(
                "{}-{}{}",
                out_path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("image"),
                index,
                out_path
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|extension| format!(".{extension}"))
                    .unwrap_or_default()
            ))
        })
        .collect())
}

fn derive_downscale_path(path: &Path, suffix: &str) -> PathBuf {
    let suffix = if suffix.is_empty() || suffix.starts_with('-') || suffix.starts_with('_') {
        suffix.to_string()
    } else {
        format!("-{suffix}")
    };
    path.with_file_name(format!(
        "{}{}{}",
        path.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("image"),
        suffix,
        path.extension()
            .and_then(|value| value.to_str())
            .map(|extension| format!(".{extension}"))
            .unwrap_or_default()
    ))
}

fn downscale_paths(output_paths: &[PathBuf], output: &OutputOptions) -> Option<Vec<PathBuf>> {
    output.downscale_max_dim?;
    Some(
        output_paths
            .iter()
            .map(|path| derive_downscale_path(path, &output.downscale_suffix))
            .collect(),
    )
}

fn ensure_output_paths_available(paths: &[PathBuf], force: bool) -> Result<()> {
    if force {
        return Ok(());
    }
    for path in paths {
        if path.exists() {
            return Err(Error::message(format!(
                "Output already exists: {} (use --force to overwrite)",
                path.display()
            )));
        }
    }
    Ok(())
}

pub fn run_generate(options: GenerateOptions) -> Result<ImageRunReport> {
    let prompt = read_prompt(options.prompt.as_deref(), options.prompt_file.as_deref())?;
    validate_generate_options(&options)?;
    let prompt = augment_prompt(options.augment, &prompt, &options.fields);
    let payload = build_generate_payload(&options, &prompt);
    let output_paths = build_output_paths(&options.output, options.n)?;
    let downscaled = downscale_paths(&output_paths, &options.output);

    if options.dry_run {
        return Ok(report(
            "generate",
            "/v1/images/generations",
            output_paths,
            downscaled,
            true,
            payload,
        ));
    }

    let (base_url, api_key) = service_configuration()?;
    let response = request_json(
        &image_api_url(&base_url, "generations")?,
        &payload,
        &api_key,
        DEFAULT_TIMEOUT_SECONDS,
        0,
        2.0,
    )?;
    save_response_images(&response, &output_paths, &downscaled, &options.output)?;
    Ok(report(
        "generate",
        "/v1/images/generations",
        output_paths,
        downscaled,
        false,
        payload,
    ))
}

pub fn run_edit(options: EditOptions) -> Result<ImageRunReport> {
    let prompt = read_prompt(
        options.generate.prompt.as_deref(),
        options.generate.prompt_file.as_deref(),
    )?;
    let image_paths = check_image_paths(&options.generate.images)?;
    let mask_path = check_mask_path(options.mask.as_deref())?;
    validate_generate_options(&options.generate)?;
    let prompt = augment_prompt(options.generate.augment, &prompt, &options.generate.fields);
    let payload = build_edit_payload(&options, &prompt)?;
    let output_paths = build_output_paths(&options.generate.output, options.generate.n)?;
    let downscaled = downscale_paths(&output_paths, &options.generate.output);

    if options.generate.dry_run {
        let mut preview = payload.clone();
        preview["image"] = json!(image_paths
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>());
        if let Some(mask_path) = &mask_path {
            preview["mask"] = json!(mask_path.to_string_lossy().to_string());
        }
        return Ok(report(
            "edit",
            "/v1/images/edits",
            output_paths,
            downscaled,
            true,
            preview,
        ));
    }

    let (base_url, api_key) = service_configuration()?;
    let (body, content_type) = build_edit_multipart(&payload, &image_paths, mask_path.as_deref())?;
    let response = request_multipart(
        &image_api_url(&base_url, "edits")?,
        &body,
        &content_type,
        &api_key,
        DEFAULT_TIMEOUT_SECONDS,
        0,
        2.0,
    )?;
    save_response_images(
        &response,
        &output_paths,
        &downscaled,
        &options.generate.output,
    )?;
    Ok(report(
        "edit",
        "/v1/images/edits",
        output_paths,
        downscaled,
        false,
        payload,
    ))
}

fn report(
    operation: &str,
    endpoint: &str,
    outputs: Vec<PathBuf>,
    downscaled: Option<Vec<PathBuf>>,
    dry_run: bool,
    payload: Value,
) -> ImageRunReport {
    ImageRunReport {
        operation: operation.to_string(),
        endpoint: endpoint.to_string(),
        outputs: outputs
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        outputs_downscaled: downscaled.map(|paths| {
            paths
                .iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect()
        }),
        dry_run,
        payload,
    }
}

fn save_response_images(
    response: &Value,
    output_paths: &[PathBuf],
    downscale_paths: &Option<Vec<PathBuf>>,
    output: &OutputOptions,
) -> Result<()> {
    let images = extract_image_outputs(response)?;
    if images.len() < output_paths.len() {
        return Err(Error::message(format!(
            "API response returned {} image(s), but {} output path(s) were expected.",
            images.len(),
            output_paths.len()
        )));
    }
    ensure_output_paths_available(output_paths, output.force)?;
    if let Some(paths) = downscale_paths {
        ensure_output_paths_available(paths, output.force)?;
    }
    let output_format = normalize_output_format(&output.output_format)?;
    for (index, image_bytes) in images.iter().enumerate() {
        if index >= output_paths.len() {
            break;
        }
        let path = &output_paths[index];
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, image_bytes)?;
        if let (Some(max_dim), Some(paths)) = (output.downscale_max_dim, downscale_paths) {
            let downscale_path = &paths[index];
            if let Some(parent) = downscale_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let resized = downscale_image_bytes(image_bytes, max_dim, &output_format)?;
            fs::write(downscale_path, resized)?;
        }
    }
    Ok(())
}

fn downscale_image_bytes(image_bytes: &[u8], max_dim: u32, output_format: &str) -> Result<Vec<u8>> {
    if max_dim < 1 {
        return Err(Error::message("--downscale-max-dim must be >= 1"));
    }
    let image = image::load_from_memory(image_bytes)?;
    let (width, height) = image.dimensions();
    let max_edge = width.max(height);
    let resized = if max_edge <= max_dim {
        image
    } else {
        let scale = max_dim as f32 / max_edge as f32;
        image.resize(
            (width as f32 * scale).round().max(1.0) as u32,
            (height as f32 * scale).round().max(1.0) as u32,
            image::imageops::FilterType::Lanczos3,
        )
    };
    let final_image = if output_format == "jpeg" {
        DynamicImage::ImageRgb8(resized.to_rgb8())
    } else {
        resized
    };
    let mut out = Cursor::new(Vec::new());
    final_image.write_to(&mut out, image_format(output_format)?)?;
    Ok(out.into_inner())
}

fn image_format(output_format: &str) -> Result<ImageFormat> {
    match output_format {
        "png" => Ok(ImageFormat::Png),
        "jpeg" => Ok(ImageFormat::Jpeg),
        "webp" => Ok(ImageFormat::WebP),
        _ => Err(Error::message(
            "output-format must be png, jpeg, jpg, or webp.",
        )),
    }
}

fn check_image_paths(paths: &[String]) -> Result<Vec<PathBuf>> {
    if paths.is_empty() {
        return Err(Error::message(
            "At least one --image is required for image edits.",
        ));
    }
    let mut resolved = Vec::new();
    for raw in paths {
        let path = expand_home(raw);
        if !path.exists() {
            return Err(Error::message(format!(
                "Image file not found: {}",
                path.display()
            )));
        }
        if path.metadata()?.len() > MAX_IMAGE_BYTES {
            eprintln!("Warning: Image exceeds 50MB limit: {}", path.display());
        }
        resolved.push(path);
    }
    Ok(resolved)
}

fn check_mask_path(mask: Option<&Path>) -> Result<Option<PathBuf>> {
    let Some(mask) = mask else {
        return Ok(None);
    };
    if !mask.exists() {
        return Err(Error::message(format!(
            "Mask file not found: {}",
            mask.display()
        )));
    }
    if mask.extension().and_then(|value| value.to_str()) != Some("png") {
        eprintln!(
            "Warning: Mask should be a PNG with an alpha channel: {}",
            mask.display()
        );
    }
    if mask.metadata()?.len() > MAX_IMAGE_BYTES {
        eprintln!("Warning: Mask exceeds 50MB limit: {}", mask.display());
    }
    Ok(Some(mask.to_path_buf()))
}

fn append_form_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(value.as_bytes());
    body.extend_from_slice(b"\r\n");
}

fn append_file_field(
    body: &mut Vec<u8>,
    boundary: &str,
    name: &str,
    filename: &str,
    content_type: &str,
    data: &[u8],
) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(b"\r\n");
}

pub fn build_edit_multipart(
    fields: &Value,
    images: &[PathBuf],
    mask: Option<&Path>,
) -> Result<(Vec<u8>, String)> {
    let fields = fields
        .as_object()
        .ok_or_else(|| Error::message("Multipart fields must be a JSON object"))?;
    let boundary = format!("----gpt-image-2-api-skill-{}", Uuid::new_v4().simple());
    let mut body = Vec::new();
    for (key, value) in fields {
        if let Some(value) = scalar_string(value) {
            if !value.is_empty() {
                append_form_field(&mut body, &boundary, key, &value);
            }
        }
    }
    for (index, image_path) in images.iter().enumerate() {
        let filename = image_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("image.png");
        let field_name = if index == 0 { "image" } else { "image[]" };
        append_file_field(
            &mut body,
            &boundary,
            field_name,
            filename,
            &content_type_for_filename(filename),
            &fs::read(image_path)?,
        );
    }
    if let Some(mask) = mask {
        let filename = mask
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("mask.png");
        append_file_field(
            &mut body,
            &boundary,
            "mask",
            filename,
            &content_type_for_filename(filename),
            &fs::read(mask)?,
        );
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((body, format!("multipart/form-data; boundary={boundary}")))
}

fn scalar_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn content_type_for_filename(filename: &str) -> String {
    match Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("tif" | "tiff") => "image/tiff",
        _ => "application/octet-stream",
    }
    .to_string()
}

pub fn run_generate_batch(options: BatchOptions) -> Result<BatchReport> {
    if !options.base.dry_run {
        service_configuration()?;
    }
    if options.concurrency < 1 || options.concurrency > 25 {
        return Err(Error::message("--concurrency must be between 1 and 25"));
    }
    if options.max_attempts < 1 || options.max_attempts > 10 {
        return Err(Error::message("--max-attempts must be between 1 and 10"));
    }
    let jobs = read_jobs_jsonl(&options.input)?;
    if options.base.dry_run {
        return run_generate_batch_sequential(&options, &jobs);
    }
    run_generate_batch_concurrent(options, jobs)
}

fn run_generate_batch_sequential(options: &BatchOptions, jobs: &[Value]) -> Result<BatchReport> {
    let mut reports = Vec::new();
    let mut failures = Vec::new();
    for (index, job) in jobs.iter().enumerate() {
        let job_number = index + 1;
        match run_batch_job(options, job_number, job) {
            Ok(report) => reports.push(report),
            Err(error) => {
                failures.push(BatchFailure {
                    job: job_number,
                    error: error.to_string(),
                });
                if options.fail_fast {
                    return Ok(BatchReport {
                        exit_code: 1,
                        dry_run: options.base.dry_run,
                        jobs: reports,
                        failures,
                    });
                }
            }
        }
    }
    Ok(BatchReport {
        exit_code: if failures.is_empty() { 0 } else { 1 },
        dry_run: options.base.dry_run,
        jobs: reports,
        failures,
    })
}

fn run_generate_batch_concurrent(options: BatchOptions, jobs: Vec<Value>) -> Result<BatchReport> {
    let job_count = jobs.len();
    let mut handles = Vec::new();
    let mut next_index = 0;
    let mut reports = Vec::new();
    let mut failures = Vec::new();

    while next_index < job_count || !handles.is_empty() {
        while next_index < job_count && handles.len() < usize::from(options.concurrency) {
            let job_number = next_index + 1;
            let job = jobs[next_index].clone();
            let worker_options = options.clone();
            eprintln!("[job {job_number}/{job_count}] starting");
            handles.push((
                job_number,
                thread::spawn(move || {
                    let started = std::time::Instant::now();
                    let result = run_batch_job(&worker_options, job_number, &job);
                    (result, started.elapsed())
                }),
            ));
            next_index += 1;
        }

        let (job_number, handle) = handles.remove(0);
        let (result, elapsed) = handle
            .join()
            .map_err(|_| Error::message(format!("Job {job_number} panicked")))?;
        match result {
            Ok(report) => {
                eprintln!(
                    "[job {job_number}/{job_count}] completed in {:.1}s",
                    elapsed.as_secs_f64()
                );
                reports.push((job_number, report));
            }
            Err(error) => {
                eprintln!("[job {job_number}/{job_count}] failed: {error}");
                failures.push(BatchFailure {
                    job: job_number,
                    error: error.to_string(),
                });
                if options.fail_fast {
                    break;
                }
            }
        }
    }

    if options.fail_fast {
        for (job_number, handle) in handles {
            let (result, elapsed) = handle
                .join()
                .map_err(|_| Error::message(format!("Job {job_number} panicked")))?;
            match result {
                Ok(report) => {
                    eprintln!(
                        "[job {job_number}/{job_count}] completed in {:.1}s",
                        elapsed.as_secs_f64()
                    );
                    reports.push((job_number, report));
                }
                Err(error) => {
                    eprintln!("[job {job_number}/{job_count}] failed: {error}");
                    failures.push(BatchFailure {
                        job: job_number,
                        error: error.to_string(),
                    });
                }
            }
        }
    }

    reports.sort_by_key(|(job_number, _)| *job_number);
    failures.sort_by_key(|failure| failure.job);
    Ok(BatchReport {
        exit_code: if failures.is_empty() { 0 } else { 1 },
        dry_run: options.base.dry_run,
        jobs: reports.into_iter().map(|(_, report)| report).collect(),
        failures,
    })
}

fn run_batch_job(options: &BatchOptions, index: usize, job: &Value) -> Result<ImageRunReport> {
    let mut current = options.base.clone();
    current.prompt = Some(job_prompt(job, index)?);
    current.prompt_file = None;
    current.output.out_dir = Some(options.out_dir.clone());
    apply_job_overrides(&mut current, job)?;
    validate_generate_options(&current)?;
    let raw_prompt = read_prompt(current.prompt.as_deref(), None)?;
    let fields = fields_for_job(&current.fields, job);
    let prompt = augment_prompt(current.augment, &raw_prompt, &fields);
    let payload = build_generate_payload(&current, &prompt);
    let output_format = normalize_output_format(&current.output.output_format)?;
    let output_paths = job_output_paths(
        &options.out_dir,
        &output_format,
        index,
        &raw_prompt,
        current.n,
        job.get("out").and_then(Value::as_str),
    )?;
    let downscaled = downscale_paths(&output_paths, &current.output);

    if current.dry_run {
        return Ok(report(
            "generate-batch",
            "/v1/images/generations",
            output_paths,
            downscaled,
            true,
            payload,
        ));
    }

    let (base_url, api_key) = service_configuration()?;
    let response = request_json(
        &image_api_url(&base_url, "generations")?,
        &payload,
        &api_key,
        DEFAULT_TIMEOUT_SECONDS,
        u32::from(options.max_attempts.saturating_sub(1)),
        2.0,
    )?;
    save_response_images(&response, &output_paths, &downscaled, &current.output)?;
    Ok(report(
        "generate-batch",
        "/v1/images/generations",
        output_paths,
        downscaled,
        false,
        payload,
    ))
}

fn read_jobs_jsonl(path: &Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Err(Error::message(format!(
            "Input file not found: {}",
            path.display()
        )));
    }
    let mut jobs = Vec::new();
    for (line_index, raw) in fs::read_to_string(path)?.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let job = if line.starts_with('{') {
            serde_json::from_str::<Value>(line).map_err(|error| {
                Error::message(format!("Invalid JSON on line {}: {error}", line_index + 1))
            })?
        } else {
            json!(line)
        };
        job_prompt(&job, line_index + 1)?;
        jobs.push(job);
    }
    if jobs.is_empty() {
        return Err(Error::message("No jobs found in input file."));
    }
    if jobs.len() > MAX_BATCH_JOBS {
        return Err(Error::message(format!(
            "Too many jobs ({}). Max is {}.",
            jobs.len(),
            MAX_BATCH_JOBS
        )));
    }
    Ok(jobs)
}

fn job_prompt(job: &Value, index: usize) -> Result<String> {
    if let Some(prompt) = job
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(prompt.to_string());
    }
    if let Some(prompt) = job
        .get("prompt")
        .map(value_to_string)
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(prompt.to_string());
    }
    Err(Error::message(format!("Missing prompt for job {index}")))
}

fn apply_job_overrides(options: &mut GenerateOptions, job: &Value) -> Result<()> {
    let Some(object) = job.as_object() else {
        return Ok(());
    };
    if let Some(value) = object.get("model").map(value_to_string) {
        options.model = value.to_string();
    }
    if let Some(value) = object.get("n").map(parse_job_u8).transpose()? {
        if !(1..=10).contains(&value) {
            return Err(Error::message("n must be between 1 and 10"));
        }
        options.n = value;
    }
    if let Some(value) = object.get("size").map(value_to_string) {
        options.size = value.to_string();
    }
    if let Some(value) = object.get("quality").map(value_to_string) {
        options.quality = value.to_string();
    }
    if let Some(value) = object.get("background").map(value_to_string) {
        options.background = Some(value.to_string());
    }
    if let Some(value) = object.get("output_format").map(value_to_string) {
        options.output.output_format = normalize_output_format(&value)?;
    }
    if let Some(value) = object
        .get("output_compression")
        .map(parse_job_u8)
        .transpose()?
    {
        if value > 100 {
            return Err(Error::message(
                "output_compression must be between 0 and 100",
            ));
        }
        options.output_compression = Some(value);
    }
    if let Some(value) = object.get("moderation").map(value_to_string) {
        options.moderation = Some(value.to_string());
    }
    Ok(())
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn parse_job_u8(value: &Value) -> Result<u8> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| Error::message("numeric job override must be between 0 and 255")),
        Value::String(value) => value
            .trim()
            .parse::<u8>()
            .map_err(|_| Error::message(format!("numeric job override is invalid: {value}"))),
        _ => Err(Error::message(
            "numeric job override must be a number or string",
        )),
    }
}

fn fields_for_job(base: &PromptFields, job: &Value) -> PromptFields {
    let mut fields = base.clone();
    if let Some(job_fields) = job.get("fields").and_then(Value::as_object) {
        merge_prompt_fields_object(&mut fields, job_fields);
    }
    if let Some(object) = job.as_object() {
        merge_prompt_fields_object(&mut fields, object);
    }
    fields
}

fn merge_prompt_fields_object(fields: &mut PromptFields, object: &Map<String, Value>) {
    macro_rules! apply {
        ($field:ident, $key:literal) => {
            if let Some(value) = object.get($key).and_then(Value::as_str) {
                fields.$field = Some(value.to_string());
            }
        };
    }
    apply!(use_case, "use_case");
    apply!(scene, "scene");
    apply!(subject, "subject");
    apply!(style, "style");
    apply!(composition, "composition");
    apply!(lighting, "lighting");
    apply!(palette, "palette");
    apply!(materials, "materials");
    apply!(text, "text");
    apply!(constraints, "constraints");
    apply!(negative, "negative");
}

fn job_output_paths(
    out_dir: &Path,
    output_format: &str,
    index: usize,
    prompt: &str,
    n: u8,
    explicit_out: Option<&str>,
) -> Result<Vec<PathBuf>> {
    let out_dir = absolutize(out_dir)?;
    let extension = format!(".{output_format}");
    let base = if let Some(out) = explicit_out {
        let mut base = PathBuf::from(out);
        if base.extension().is_none() {
            base = base.with_extension(output_format);
        }
        out_dir.join(
            base.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("image.png"),
        )
    } else {
        out_dir.join(format!("{index:03}-{}{}", slugify(prompt), extension))
    };
    if n == 1 {
        return Ok(vec![base]);
    }
    Ok((1..=n)
        .map(|variant| {
            base.with_file_name(format!(
                "{}-{}{}",
                base.file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("image"),
                variant,
                base.extension()
                    .and_then(|value| value.to_str())
                    .map(|extension| format!(".{extension}"))
                    .unwrap_or_default()
            ))
        })
        .collect())
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
        if slug.len() >= 60 {
            break;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "job".to_string()
    } else {
        slug
    }
}

fn read_response_bytes(response: ureq::Response) -> Result<Vec<u8>> {
    let mut reader = response.into_reader();
    let mut data = Vec::new();
    reader.read_to_end(&mut data)?;
    Ok(data)
}

pub fn error_message_from_body(body: &[u8]) -> String {
    if body.is_empty() {
        return String::new();
    }
    let Ok(payload) = serde_json::from_slice::<Value>(body) else {
        return String::from_utf8_lossy(body).chars().take(1000).collect();
    };
    if let Some(error) = payload.get("error") {
        if let Some(message) = error.get("message").and_then(Value::as_str) {
            return message.to_string();
        }
        if let Some(error) = error.as_str() {
            return error.to_string();
        }
    }
    serde_json::to_string(&payload)
        .unwrap_or_default()
        .chars()
        .take(1000)
        .collect()
}

fn http_agent(timeout_seconds: u64) -> Result<ureq::Agent> {
    let timeout = Duration::from_secs(timeout_seconds);
    Ok(ureq::AgentBuilder::new()
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .timeout_write(timeout)
        .build())
}

pub fn request_bytes(
    url: &str,
    data: &[u8],
    headers: &[(&str, String)],
    timeout_seconds: u64,
    retries: u32,
    retry_delay_seconds: f64,
) -> Result<Vec<u8>> {
    let agent = http_agent(timeout_seconds)?;
    let mut last_error = None;
    for attempt in 0..=retries {
        let mut request = agent.post(url);
        for (key, value) in headers {
            request = request.set(key, value);
        }
        match request.send_bytes(data) {
            Ok(response) => return read_response_bytes(response),
            Err(ureq::Error::Status(status, response)) => {
                let body = read_response_bytes(response).unwrap_or_default();
                let message = format!("HTTP {status}: {}", error_message_from_body(&body));
                if !RETRYABLE_HTTP_STATUSES.contains(&status) || attempt >= retries {
                    return Err(Error::message(message));
                }
                last_error = Some(message);
            }
            Err(error) => {
                let message = format!("Network error: {error}");
                if attempt >= retries {
                    return Err(Error::message(message));
                }
                last_error = Some(message);
            }
        }
        if retry_delay_seconds > 0.0 {
            thread::sleep(Duration::from_secs_f64(retry_delay_seconds));
        }
    }
    Err(Error::message(
        last_error.unwrap_or_else(|| "API request failed".to_string()),
    ))
}

pub fn request_json(
    url: &str,
    payload: &Value,
    api_key: &str,
    timeout_seconds: u64,
    retries: u32,
    retry_delay_seconds: f64,
) -> Result<Value> {
    let body = serde_json::to_vec(payload)?;
    let headers = vec![
        ("Authorization", format!("Bearer {api_key}")),
        ("Content-Type", "application/json".to_string()),
    ];
    let response_body = request_bytes(
        url,
        &body,
        &headers,
        timeout_seconds,
        retries,
        retry_delay_seconds,
    )?;
    parse_json_response(&response_body)
}

pub fn request_multipart(
    url: &str,
    body: &[u8],
    content_type: &str,
    api_key: &str,
    timeout_seconds: u64,
    retries: u32,
    retry_delay_seconds: f64,
) -> Result<Value> {
    let headers = vec![
        ("Authorization", format!("Bearer {api_key}")),
        ("Content-Type", content_type.to_string()),
    ];
    let response_body = request_bytes(
        url,
        body,
        &headers,
        timeout_seconds,
        retries,
        retry_delay_seconds,
    )?;
    parse_json_response(&response_body)
}

pub fn parse_json_response(body: &[u8]) -> Result<Value> {
    let payload: Value = serde_json::from_slice(body)
        .map_err(|_| Error::message("API response was not valid JSON"))?;
    if !payload.is_object() {
        return Err(Error::message("API response JSON must be an object"));
    }
    Ok(payload)
}

pub fn image_object_candidates(response: &Value) -> Vec<&Value> {
    let mut candidates = Vec::new();
    if let Some(data) = response.get("data").and_then(Value::as_array) {
        candidates.extend(data.iter().filter(|item| item.is_object()));
    }
    candidates
}

pub fn extract_image_outputs(response: &Value) -> Result<Vec<Vec<u8>>> {
    let mut outputs = Vec::new();
    for item in image_object_candidates(response) {
        if let Some(b64_image) = item
            .get("b64_json")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            outputs.push(STANDARD.decode(b64_image).map_err(|error| {
                Error::message(format!("Image b64_json was not valid base64: {error}"))
            })?);
        }
    }
    if outputs.is_empty() {
        Err(Error::message(
            "API response did not include data[0].b64_json",
        ))
    } else {
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn output_options(path: &Path) -> OutputOptions {
        OutputOptions {
            out: path.to_path_buf(),
            out_dir: None,
            output_format: "png".to_string(),
            force: false,
            downscale_max_dim: None,
            downscale_suffix: DEFAULT_DOWNSCALE_SUFFIX.to_string(),
        }
    }

    fn generate_options(prompt: &str, output: &Path) -> GenerateOptions {
        GenerateOptions {
            prompt: Some(prompt.to_string()),
            prompt_file: None,
            images: Vec::new(),
            output: output_options(output),
            model: DEFAULT_MODEL.to_string(),
            n: 1,
            size: DEFAULT_SIZE.to_string(),
            quality: DEFAULT_QUALITY.to_string(),
            background: None,
            output_compression: None,
            moderation: None,
            augment: true,
            fields: PromptFields::default(),
            dry_run: false,
        }
    }

    #[test]
    fn validates_gpt_image_2_size_constraints() {
        assert!(validate_size("3840x2160", "gpt-image-2").is_ok());
        assert!(validate_size("2160x3840", "gpt-image-2").is_ok());
        assert!(validate_size("3841x2160", "gpt-image-2").is_err());
        assert!(validate_size("1025x1024", "gpt-image-2").is_err());
        assert!(validate_size("4096x512", "gpt-image-2").is_err());
    }

    #[test]
    fn augments_prompt_like_imagegen_cli() {
        let fields = PromptFields {
            use_case: Some("product-mockup".to_string()),
            style: Some("clean product photography".to_string()),
            constraints: Some("no logos".to_string()),
            ..PromptFields::default()
        };
        assert_eq!(
            augment_prompt(true, "A ceramic mug", &fields),
            "Use case: product-mockup\nPrimary request: A ceramic mug\nStyle/medium: clean product photography\nConstraints: no logos"
        );
        assert_eq!(
            augment_prompt(false, "A ceramic mug", &fields),
            "A ceramic mug"
        );
    }

    #[test]
    fn dry_run_generate_returns_payload_and_outputs_without_config() {
        let temp_dir = TempDir::new();
        let output = temp_dir.path().join("image.png");
        let mut options = generate_options("cat", &output);
        options.dry_run = true;
        options.quality = "high".to_string();
        options.size = "2048x1152".to_string();

        let report = run_generate(options).unwrap();
        assert!(report.dry_run);
        assert_eq!(report.payload["quality"], "high");
        assert_eq!(report.payload["size"], "2048x1152");
        assert_eq!(report.outputs, vec![output.to_string_lossy().to_string()]);
    }

    #[test]
    fn output_paths_match_imagegen_numbering() {
        let temp_dir = TempDir::new();
        let out = temp_dir.path().join("hero.png");
        let mut options = output_options(&out);
        options.output_format = "png".to_string();
        assert_eq!(
            build_output_paths(&options, 3).unwrap(),
            vec![
                temp_dir.path().join("hero-1.png"),
                temp_dir.path().join("hero-2.png"),
                temp_dir.path().join("hero-3.png")
            ]
        );
        options.out_dir = Some(temp_dir.path().join("batch"));
        assert_eq!(
            build_output_paths(&options, 2).unwrap(),
            vec![
                temp_dir.path().join("batch/image_1.png"),
                temp_dir.path().join("batch/image_2.png")
            ]
        );
    }

    #[test]
    fn edit_multipart_includes_image_array_and_mask() {
        let temp_dir = TempDir::new();
        let image_a = temp_dir.path().join("a.png");
        let image_b = temp_dir.path().join("b.png");
        let mask = temp_dir.path().join("mask.png");
        fs::write(&image_a, b"a").unwrap();
        fs::write(&image_b, b"b").unwrap();
        fs::write(&mask, b"m").unwrap();
        let (body, content_type) = build_edit_multipart(
            &json!({"model": "gpt-image-1.5", "prompt": "edit", "n": 1, "quality": "high"}),
            &[image_a, image_b],
            Some(&mask),
        )
        .unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        assert_eq!(text.matches("name=\"image\"").count(), 1);
        assert_eq!(text.matches("name=\"image[]\"").count(), 1);
        assert!(text.contains("name=\"mask\""));
        assert!(text.contains("name=\"quality\""));
    }

    #[test]
    fn run_generate_saves_response_image() {
        let temp_dir = TempDir::new();
        let output = temp_dir.path().join("image.png");
        let response_body = json!({
            "created": 123,
            "data": [{"b64_json": STANDARD.encode(b"image")}]
        })
        .to_string();
        let server = TestServer::spawn(move |request| {
            assert!(request.contains("POST /v1/images/generations HTTP/1.1"));
            assert!(request.contains("\"quality\":\"medium\""));
            HttpResponse {
                status: 200,
                body: response_body.clone(),
                content_type: "application/json",
            }
        });
        let _env = ServiceEnvGuard::set(&server.url(), "test-key");
        let options = generate_options("cat", &output);
        let report = run_generate(options).unwrap();

        assert_eq!(fs::read(output).unwrap(), b"image");
        assert!(!report.dry_run);
        assert_eq!(report.payload["quality"], "medium");
    }

    #[test]
    fn batch_dry_run_reads_string_and_object_jobs() {
        let temp_dir = TempDir::new();
        let input = temp_dir.path().join("jobs.jsonl");
        fs::write(
            &input,
            "A cat\n{\"prompt\":\"A dog\",\"size\":\"1024x1024\",\"out\":\"dog.png\"}\n",
        )
        .unwrap();
        let mut base = generate_options("base", &temp_dir.path().join("unused.png"));
        base.dry_run = true;
        base.output.out_dir = Some(temp_dir.path().join("out"));
        let report = run_generate_batch(BatchOptions {
            base,
            input,
            out_dir: temp_dir.path().join("out"),
            concurrency: 5,
            max_attempts: 3,
            fail_fast: false,
        })
        .unwrap();
        assert_eq!(report.exit_code, 0);
        assert_eq!(report.jobs.len(), 2);
        assert_eq!(report.jobs[1].payload["size"], "1024x1024");
        assert!(report.jobs[1].outputs[0].ends_with("dog.png"));
    }

    #[test]
    fn batch_slug_uses_raw_prompt_and_normalizes_string_overrides() {
        let temp_dir = TempDir::new();
        let input = temp_dir.path().join("jobs.jsonl");
        fs::write(
            &input,
            "{\"prompt\":\"A cat\",\"n\":\"2\",\"output_format\":\"WEBP\",\"output_compression\":\"80\"}\n",
        )
        .unwrap();
        let mut base = generate_options("base", &temp_dir.path().join("unused.png"));
        base.dry_run = true;
        let report = run_generate_batch(BatchOptions {
            base,
            input,
            out_dir: temp_dir.path().join("out"),
            concurrency: 5,
            max_attempts: 3,
            fail_fast: false,
        })
        .unwrap();

        assert_eq!(report.jobs[0].payload["output_format"], "webp");
        assert_eq!(report.jobs[0].payload["output_compression"], 80);
        assert_eq!(
            report.jobs[0].outputs,
            vec![
                temp_dir
                    .path()
                    .join("out/001-a-cat-1.webp")
                    .to_string_lossy()
                    .to_string(),
                temp_dir
                    .path()
                    .join("out/001-a-cat-2.webp")
                    .to_string_lossy()
                    .to_string()
            ]
        );
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let path = env::temp_dir().join(format!("gpt-image-2-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    struct ServiceEnvGuard {
        old_base_url: Option<String>,
        old_api_key: Option<String>,
    }

    impl ServiceEnvGuard {
        fn set(base_url: &str, api_key: &str) -> Self {
            let old_base_url = env::var(BASE_URL_ENV_NAME).ok();
            let old_api_key = env::var(API_KEY_ENV_NAME).ok();
            env::set_var(BASE_URL_ENV_NAME, base_url);
            env::set_var(API_KEY_ENV_NAME, api_key);
            Self {
                old_base_url,
                old_api_key,
            }
        }
    }

    impl Drop for ServiceEnvGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.old_base_url {
                env::set_var(BASE_URL_ENV_NAME, value);
            } else {
                env::remove_var(BASE_URL_ENV_NAME);
            }
            if let Some(value) = &self.old_api_key {
                env::set_var(API_KEY_ENV_NAME, value);
            } else {
                env::remove_var(API_KEY_ENV_NAME);
            }
        }
    }

    struct HttpResponse {
        status: u16,
        body: String,
        content_type: &'static str,
    }

    struct TestServer {
        address: String,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl TestServer {
        fn spawn<F>(handler: F) -> Self
        where
            F: Fn(String) -> HttpResponse + Send + 'static,
        {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let handle = thread::spawn(move || {
                for stream in listener.incoming().take(1) {
                    let Ok(mut stream) = stream else {
                        continue;
                    };
                    let request = read_http_request(&mut stream);
                    let response = handler(request);
                    let http = format!(
                        "HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.status,
                        response.content_type,
                        response.body.len(),
                        response.body
                    );
                    stream.write_all(http.as_bytes()).unwrap();
                }
            });
            Self {
                address,
                handle: Some(handle),
            }
        }

        fn url(&self) -> String {
            format!("http://{}", self.address)
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> String {
        let mut data = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let bytes_read = stream.read(&mut buffer).unwrap();
            if bytes_read == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..bytes_read]);
            let header_end = find_header_end(&data);
            if let Some(header_end) = header_end {
                let headers = String::from_utf8_lossy(&data[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                if data.len() >= header_end + 4 + content_length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&data).to_string()
    }

    fn find_header_end(data: &[u8]) -> Option<usize> {
        data.windows(4).position(|window| window == b"\r\n\r\n")
    }
}
