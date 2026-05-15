# gpt-image-2 Skill

Rust-only Codex skill and CLI for a GPT Image compatible API gateway. The CLI surface mirrors the system `imagegen` fallback CLI: `generate`, `edit`, and `generate-batch`.

## What It Provides

- `gpt-image-2 generate` for text-to-image requests
- `gpt-image-2 edit` for multipart image edits
- `gpt-image-2 generate-batch` for JSONL batch generation
- Prompt augmentation fields, downscaled copies, and dry-run reports
- Local output saving from `data[].b64_json`
- A bundled `references/` prompt gallery and craft checklist for reference-assisted prompt construction

## Install CLI

```bash
cargo install --path .
```

Or build without installing:

```bash
cargo build --release
./target/release/gpt-image-2 --help
```

## Configure

```bash
export GPT_IMAGE_2_BASE_URL="http://<api-host>:<port>"
export BASE_URL_API_KEY="<API_KEY>"
```

Optional defaults:

```bash
export IMAGE_MODEL="gpt-image-2"
export IMAGE_N="1"
export IMAGE_SIZE="auto"
export IMAGE_QUALITY="medium"
export IMAGE_OUTPUT_FORMAT="png"
export IMAGE_OUTPUT="output/imagegen/output.png"
export IMAGE_DOWNSCALE_SUFFIX="-web"
```

## Defaults

- Model: `gpt-image-2`
- Size: `auto`
- Quality: `medium`
- Output format: `png`
- Output path: `output/imagegen/output.png`
- Batch concurrency: `5`
- Downscale suffix: `-web`
- `n`: `1`, validated as `1` through `10`

## Generate

```bash
gpt-image-2 generate \
  --prompt "一只戴墨镜的橘猫，赛博朋克风" \
  --size 1024x1024 \
  --quality high \
  --out output/imagegen/cat.png
```

4K landscape example:

```bash
gpt-image-2 generate \
  --prompt "16:9 4K output: 3840x2160 Widescreen 4K output. A cinematic neon city skyline at night." \
  --size 3840x2160 \
  --quality high \
  --out output/imagegen/city-4k.png
```

Dry-run prints the computed payload and output paths without calling the API:

```bash
gpt-image-2 generate \
  --prompt "A cyberpunk orange cat" \
  --out output/imagegen/cat.png \
  --dry-run
```

## Edit Images

```bash
gpt-image-2 edit \
  --image input.png \
  --prompt "把图片改成赛博朋克霓虹风，保留主体轮廓" \
  --quality high \
  --out output/imagegen/edited.png
```

For multi-image edits, repeat `--image`. The first file is sent as `image`; later files are sent as `image[]`. `--mask` is edit-only. `--input-fidelity low|high` is edit-only and is rejected for `gpt-image-2`, matching `imagegen`.

## Generate Batch

```bash
mkdir -p tmp/imagegen output/imagegen/batch
cat > tmp/imagegen/prompts.jsonl << 'EOF'
{"prompt":"Cavernous hangar interior with a compact shuttle parked near the center","use_case":"stylized-concept","composition":"wide-angle, low-angle","size":"1536x1024"}
{"prompt":"Gray wolf in profile in a snowy forest","use_case":"photorealistic-natural","size":"1024x1024","out":"wolf.png"}
EOF

gpt-image-2 generate-batch \
  --input tmp/imagegen/prompts.jsonl \
  --out-dir output/imagegen/batch \
  --concurrency 5 \
  --max-attempts 3
```

JSONL lines can be strings or objects. Per-job overrides include `model`, `n`, `size`, `quality`, `background`, `output_format`, `output_compression`, `moderation`, `out`, and prompt augmentation fields.

## Prompt Augmentation

Augmentation is enabled by default. Use only fields that help the image:

```bash
gpt-image-2 generate \
  --prompt "A minimal hero image of a ceramic coffee mug" \
  --use-case product-mockup \
  --style "clean product photography" \
  --composition "wide product shot with usable negative space for page copy" \
  --constraints "no logos, no text" \
  --out output/imagegen/mug-hero.png
```

Pass `--no-augment` to send the prompt unchanged. Resolution is controlled by `--size`; this CLI does not infer or append 1K/2K/4K descriptors from prompt text.

There is no `--response-format b64_json` flag. The gateway response is expected to include `data[].b64_json`, and the CLI writes decoded image bytes to the output path.

## Reference-Assisted Prompting

The skill includes a local prompt reference library:

- `references/gallery.md` routes to category-specific prompt galleries.
- `references/gallery-*.md` files contain concrete visual prompt patterns.
- `references/craft.md` summarizes cross-cutting prompt techniques.
- `references/openai-cookbook.md` is a local official-reference copy for model and prompting semantics.

Use these files before high-polish, structured, text-heavy, multi-panel, UI, data, diagram, or underspecified image requests. Load the smallest relevant slice: usually `gallery.md`, one matching category file, and `craft.md` if the prompt needs repair or structure.

The references guide prompt wording only. Execution remains Rust-only through `gpt-image-2 generate`, `edit`, or `generate-batch`.

## Shared Parameters

Use these for `generate`, `edit`, and `generate-batch`:

- `--model`
- `--prompt` or `--prompt-file`
- `--n` (`1` through `10`)
- `--size`
- `--quality low|medium|high|auto`
- `--background transparent|opaque|auto`
- `--output-format png|jpeg|jpg|webp`
- `--output-compression 0..100`
- `--moderation auto|low`
- `--out`
- `--out-dir`
- `--force`
- `--dry-run`
- `--augment` / `--no-augment`
- `--downscale-max-dim`
- `--downscale-suffix`

Prompt augmentation fields:

- `--use-case`
- `--scene`
- `--subject`
- `--style`
- `--composition`
- `--lighting`
- `--palette`
- `--materials`
- `--text`
- `--constraints`
- `--negative`

Edit-only parameters:

- Repeated `--image`
- `--mask`
- `--input-fidelity low|high`

Batch-only parameters:

- `--input`
- `--out-dir` is required
- `--concurrency`
- `--max-attempts`
- `--fail-fast`

## gpt-image-2 Sizes

`gpt-image-2` accepts `auto` or `WIDTHxHEIGHT` values that satisfy all constraints:

- Maximum edge `<= 3840px`
- Both edges multiples of `16px`
- Long edge to short edge ratio `<= 3:1`
- Total pixels from `655,360` through `8,294,400`

Popular sizes:

| Label        | Size        |
|--------------|-------------|
| Square       | `1024x1024` |
| Landscape    | `1536x1024` |
| Portrait     | `1024x1536` |
| 2K square    | `2048x2048` |
| 2K landscape | `2048x1152` |
| 4K landscape | `3840x2160` |
| 4K portrait  | `2160x3840` |
| Auto         | `auto`      |

Older GPT Image models are restricted to `1024x1024`, `1536x1024`, `1024x1536`, or `auto`.

## Transparency

`gpt-image-2` rejects `--background transparent`. For true transparent CLI output, use `gpt-image-1.5` with `--background transparent --output-format png` only when that fallback is intentional.

## Main Files

- `SKILL.md` - Codex skill instructions
- `Cargo.toml` - Rust package and binary manifest
- `src/main.rs` - CLI argument parsing and command dispatch
- `src/lib.rs` - API client, multipart upload, `b64_json` response decoding, batch execution, and tests
- `references/` - prompt gallery, craft checklist, and local API/model reference notes
- `.env.example` - local configuration template

## Development

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Tests use local mock HTTP handlers and do not require live credentials.

## Security

Never commit real API keys, generated secrets, local `.env` files, or generated images unless they are intentional documentation assets.
