# Agent logos

Shown next to the coding agent, model provider or model they belong to
(`AgentLogo` in `src/agent_logo.rs`; which program, provider id or model id
gets which logo is `src/agent_logo/mapping.rs`). 128×128 PNGs with a
transparent background, the mark trimmed and centred in 120 px. The website
uses copies in `web/assets/img/ai/`.

The names and logos are trademarks of their owners. Ferrum uses them only to
identify the products it works with (the "works with" use their guidelines
allow); no endorsement is implied. Marks are not recoloured, redrawn,
stretched or combined: only scaled, cropped to the symbol (no wordmark) and,
for black monochrome marks, given the owner's own white/light version for
dark themes (inverted where the owner publishes only the black one).

`*-light.png` is for light themes and `*-dark.png` for dark ones; a logo with
one file is used on both.

| File | Logo | Source | Notes |
| --- | --- | --- | --- |
| claude.png | Claude (Anthropic): Claude, Claude Code, `anthropic` | supplied by the owner | Anthropic trademark guidelines: <https://www.anthropic.com/legal/trademark-guidelines> (business use: marketing@anthropic.com). |
| openai-light.png, openai-dark.png | OpenAI Blossom: OpenAI models, ChatGPT and Codex | supplied by the owner | <https://openai.com/brand>: Blossom in black or white only, no added colour, no stretching, cropping or effects, don't pair with the wordmark. OpenAI publishes no separate Codex mark (the Codex app icon is not offered for download, checked 2026-11-16), so Codex uses the Blossom. |
| gemini.png | Gemini (Google) | supplied by the owner | Google brand terms: <https://about.google/brand-resource-center/rules>: no alteration beyond resizing, keep clear space. |
| opencode-light.png, opencode-dark.png | OpenCode | supplied by the owner | |
| muse.png | Muse (Meta), also Llama models | supplied by the owner | |
| deepseek.png | DeepSeek | supplied by the owner | |
| kimi.png | Kimi (Moonshot AI) | supplied by the owner | |
| qwen.png | Qwen (Alibaba Cloud) | <https://assets.alicdn.com/g/qwenweb/qwen-chat-fe/0.3.43/static/images/qwen-logo.svg> (served by chat.qwen.ai), 2026-11-16 | The symbol only: the SVG's view box cropped to the mark, the "Qwen" wordmark left out. No public brand guidelines found. |
| zai.png | GLM (Z.ai, formerly Zhipu AI) | <https://z-cdn.chatglm.cn/z-ai/static/logo.svg> (served by z.ai), 2026-11-16 | The Z app tile as served. No public brand guidelines found. |
| mistral.png | Mistral AI | <https://mistral.ai/images/logos/logo-mistral.svg> (mistral.ai's header), 2026-11-16 | The pixel "M" icon cut from the lockup, gradient colours unchanged. <https://mistral.ai/brand>: partners may use the brand; no stretching, recolouring, frames or layout changes; prefer the gradient. (The brand-kit zip is behind a bot check, so the site's own SVG was used.) |
| ollama-light.png, ollama-dark.png | Ollama | <https://ollama.com/public/ollama.png>, 2026-11-16 | Black llama; the dark-theme file is the same image inverted (as ollama.com does in dark mode). No public brand guidelines found. |
| cursor-light.png, cursor-dark.png | Cursor (Cursor Agent) | <https://cursor.com/brand> kit `cursor-brand-assets.zip`, `General Logos/Cube/SVG/CUBE_2D_LIGHT.svg` and `CUBE_2D_DARK.svg`, 2026-11-16 | Cube mark in 2D (the default), light and dark versions from the kit. "Refer to us as Cursor." |
| amp.png | Amp (Sourcegraph) | <https://ampcode.com/favicon.svg>, 2026-11-16 | The app icon as served. No public brand guidelines found. |
| goose-light.png, goose-dark.png | goose (Block) | <https://goose-docs.ai/img/logo_light.png> and `logo_dark.png`, 2026-11-16 | The project's own light and dark marks. Apache-2.0 project; no separate brand guidelines found. |

Agents with no symbol of their own (aider's logo is a wordmark, Crush's an
illustration) and gateways that serve anyone's models (OpenRouter, Groq,
vLLM, LM Studio, OpenAI-compatible servers) get the plain `>_` tile, or the
model maker's logo when the model id names one.

SVG sources were sanitised (scripts, `foreignObject`, event handlers and
external references removed) and rendered with resvg 0.46; PNG sources were
trimmed and resized with Lanczos filtering.
