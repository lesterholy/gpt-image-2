---
name: gpt-image-2
description: Use this skill whenever the user asks how to call or run GPT Image generation or editing APIs through the GPT-Image-2 gateway, mentions gpt-image-2, /v1/images/generations, /v1/images/edits, Rust CLI usage, image prompt parameters, quality, size, masks, batch generation, or asks for curl/CLI examples for image generation. Prefer this skill even if the user only says "生图怎么调", "测试生图", "图片接口", "生成图片", "画图", "改图", "修图", "图片合成", "生成海报", or "生成头像".
---

# GPT-Image-2 API Skill

Use this skill to answer or run GPT Image generation and editing through the repository's Rust CLI. The CLI is intentionally aligned with the system `imagegen` fallback CLI parameter surface.

Before drafting final prompts, use the bundled `references/` gallery and craft files as a small local prompt-pattern library. This skill should behave like a reference-assisted prompt operator, not a bare CLI wrapper.

## Operating Loop

1. Classify the request as `generate`, `edit`, or `generate-batch`; identify asset type, exact in-image text, aspect ratio, reference/edit images, safety constraints, quality, and output path.
2. Search references first when the request is creative, visual-quality-sensitive, structured, text-heavy, multi-panel, or underspecified:
   - Start with `references/gallery.md` as the routing index.
   - Load exactly one closest `references/gallery-*.md` file for normal requests.
   - Load two or three category files only for explicit hybrid styles.
   - Read actual `Prompt` text before adapting a pattern.
3. Refine with `references/craft.md` for dense text, diagrams, UI, data visualization, multi-panel layouts, weak prompts, or no close gallery match.
4. Load `references/openai-cookbook.md` only for API/model capability questions, parameter semantics, or behavior uncertainty.
5. Confer when useful before expensive or ambiguous high-polish calls: present 1-3 matched directions plus planned size/quality, then ask at most one concise question. Skip this for precise "generate now" requests.
6. Execute only via the Rust `gpt-image-2` CLI or `cargo run -- ...` from this repository. Do not create or invoke alternate launchers or ad-hoc SDK scripts.
7. Report output path(s), key flags, and the reference/craft pattern used when it materially shaped the prompt.

Fast path: precise prompt plus explicit "generate now" means quick reference/craft check, then run the Rust CLI.

## Endpoints

- Generate: `POST /v1/images/generations`
- Edit: `POST /v1/images/edits`

The CLI reads the gateway base URL from `GPT_IMAGE_2_BASE_URL`, accepts it with or without `/v1`, and constructs the endpoint path.

## Configuration

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

## CLI Commands

The CLI exposes the same top-level commands as `imagegen` fallback mode:

- `generate`
- `edit`
- `generate-batch`

Examples:

```bash
gpt-image-2 generate \
  --prompt "一只戴墨镜的橘猫，赛博朋克风" \
  --size 1024x1024 \
  --quality high \
  --out output/imagegen/cat.png
```

```bash
gpt-image-2 edit \
  --image input.png \
  --prompt "把图片改成赛博朋克霓虹风，保留主体轮廓" \
  --quality high \
  --out output/imagegen/edited.png
```

```bash
gpt-image-2 generate-batch \
  --input tmp/imagegen/prompts.jsonl \
  --out-dir output/imagegen/batch \
  --concurrency 5 \
  --max-attempts 3
```

If the binary is not installed, run from the skill directory:

```bash
cargo run -- generate --prompt "A cyberpunk orange cat" --out output/imagegen/cat.png
```

## Defaults

- Model: `gpt-image-2`
- Size: `auto`
- Quality: `medium`
- Output format: `png`
- One-off output path: `output/imagegen/output.png`
- Batch concurrency: `5`
- Downscale suffix: `-web`
- Prompt augmentation: enabled by default

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

There is no `--response-format b64_json` flag. The gateway response must include `data[].b64_json`.

## Prompt Behavior

Prompt augmentation follows `imagegen`:

- If augmentation is enabled, the final prompt is structured with labeled lines such as `Use case`, `Primary request`, `Style/medium`, `Constraints`, and `Avoid`.
- If `--no-augment` is passed, the prompt is sent unchanged.
- The CLI does not infer or append 1K/2K/4K descriptors from free-form prompt text. Use `--size` to request an explicit output resolution.

If the user explicitly asks for a resolution tier in the prompt, make it explicit in the final prompt and set `--size` accordingly when the aspect ratio is known. For example, for 16:9 4K output use:

```text
3840x2160 Widescreen 4K output
```

and pass:

```bash
--size 3840x2160
```

## Reference-Assisted Prompting

Use the reference files to shape prompts, not to add unrelated content. Keep the user's intent as the source of truth.

Reference loading policy:

- `references/gallery.md`: routing index for the bundled prompt gallery. Load first.
- `references/gallery-*.md`: concrete prompts and category patterns. Load the smallest useful slice.
- `references/craft.md`: cross-cutting checklist for prompt repair and structured visual tasks.
- `references/openai-cookbook.md`: local official-reference copy for parameter/model behavior.

Prompt adaptation rules:

- Preserve exact user-supplied text verbatim and wrap displayed text in quotes.
- Put canvas, aspect ratio, and layout before subject when layout matters.
- Use structured JSON/config-style prompt blocks for premium product renders, food, complex material/lighting systems, and reusable commercial specs.
- Use fixed-region schemas for infographics, educational boards, UI mockups, data figures, and multi-panel layouts.
- For edits, explicitly state what changes and what must stay invariant.
- For multi-reference edits, identify each input by index and role: target, style reference, product/logo source, background, mask.
- Preserve `Curated` vs `Author + Source` metadata only when adding or promoting gallery entries; normal image generation does not need metadata in the final prompt.

## gpt-image-2 Sizes

`gpt-image-2` accepts `auto` or any `WIDTHxHEIGHT` value that satisfies all constraints:

- Maximum edge `<= 3840px`
- Both edges multiples of `16px`
- Long edge to short edge ratio `<= 3:1`
- Total pixels from `655,360` through `8,294,400`

Popular sizes:

| Label        | Size        | Notes                |
| ------------ | ----------- | -------------------- |
| Square       | `1024x1024` | Typical fast default |
| Landscape    | `1536x1024` | Standard landscape   |
| Portrait     | `1024x1536` | Standard portrait    |
| 2K square    | `2048x2048` | Larger square output |
| 2K landscape | `2048x1152` | Widescreen output    |
| 4K landscape | `3840x2160` | Widescreen 4K output |
| 4K portrait  | `2160x3840` | Vertical 4K output   |
| Auto         | `auto`      | Default size         |

Older GPT Image models support only `1024x1024`, `1536x1024`, `1024x1536`, or `auto`.

## Model-Specific Rules

- `gpt-image-2` rejects `--background transparent`.
- `gpt-image-2` rejects `--input-fidelity`; image inputs are always high fidelity.
- Transparent background output requires a model that supports it, usually `gpt-image-1.5`, plus `--background transparent --output-format png` or `webp`.
- `--background transparent` requires `--output-format png` or `webp`.

## Batch JSONL

Each line can be a plain prompt string or a JSON object:

```jsonl
A cat on a neon street
{"prompt":"A dog in a snowy forest","size":"1024x1024","quality":"high","out":"dog.png"}
```

Per-job overrides include `model`, `n`, `size`, `quality`, `background`, `output_format`, `output_compression`, `moderation`, `out`, and prompt augmentation fields.

## Curl Examples

Use the Rust CLI for normal work. Use curl only for gateway debugging:

```bash
curl "$GPT_IMAGE_2_BASE_URL/v1/images/generations" \
  -H "Authorization: Bearer $BASE_URL_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-image-2",
    "prompt": "一只戴墨镜的橘猫，赛博朋克风",
    "n": 1,
    "size": "auto",
    "quality": "medium",
    "output_format": "png"
  }'
```

```bash
curl "$GPT_IMAGE_2_BASE_URL/v1/images/edits" \
  -H "Authorization: Bearer $BASE_URL_API_KEY" \
  -F model="gpt-image-2" \
  -F prompt="把这张图改成赛博朋克夜景风格" \
  -F n="1" \
  -F size="auto" \
  -F quality="medium" \
  -F output_format="png" \
  -F "image=@./input.png"
```

## Troubleshooting

- If configuration is missing, set `GPT_IMAGE_2_BASE_URL` and `BASE_URL_API_KEY`.
- If output exists, add `--force` or choose another `--out`.
- If a prompt needs exact text in the image, pass it with `--text`.
- For `gpt-image-2`, do not pass `--background transparent` or `--input-fidelity`.
- For batch work, provide `--out-dir`.
