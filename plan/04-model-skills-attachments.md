# 4. Model Selector, Skills Picker & Drag-and-Drop File Attachments (Items #5, #10, #11)

## Model & Thinking Selector (#10)
- **Pi RPC Integration (`src/lib/stores/chat.svelte.ts`)**:
  - On session connect (after `get_state`), invoke `{"type": "get_available_models"}` and `{"type": "get_available_thinking_levels"}` over Pi RPC and store `availableModels: PiFullModelInfo[]` and `availableThinkingLevels: string[]`.
  - Add Rust fallback command `list_configured_models` that reads `~/.pi/agent/models.json` and `~/.pi/agent/settings.json` so model selection works before a session starts and passes `--model <provider/id>` on launch.
- **UI (`ModelSelector.svelte`)**:
  - Searchable popover in `SessionHeader.svelte` and `ChatInput.svelte` grouped by provider, showing context window size, image support badge, reasoning badge, and $/1M token pricing.
  - Selecting a model sends `{"type": "set_model", "provider": ..., "modelId": ...}` and refreshes available thinking levels; selecting a thinking level sends `{"type": "set_thinking_level", "level": ...}`.

## Installed Skills Picker (#5)
- **Skill Discovery (`src-tauri/src/skills.rs` + Pi RPC `get_commands`)**:
  - Query Pi RPC `{"type": "get_commands"}` when connected to list all discovered skills (`source === "skill"`), prompt templates (`source === "prompt"`), and extension commands (`source === "extension"`).
  - Also provide a Rust command `list_installed_skills(cwd: Option<String>)` that scans `~/.pi/agent/skills/**/SKILL.md`, `~/.agents/skills/**/SKILL.md`, and `<cwd>/.pi/skills/**/SKILL.md` (parsing YAML frontmatter `name` and `description`) so skills are always browsable.
- **UI (`SkillPickerPopover.svelte` in `ChatInput.svelte`)**:
  - Add a `Skills` button in the composer toolbar and `/` slash-command autocomplete trigger when typing `/`.
  - Selecting a skill adds a removable `/skill:<name>` pill (or inserts `/skill:<name> ` into the prompt) so Pi expands the skill on submission.

## Drag-and-Drop Files into Prompt (#11)
- **File Drop & Attachment Pipeline (`ChatInput.svelte` + `src-tauri/src/files.rs`)**:
  - Listen to both HTML5 drag-and-drop events and Tauri's window drag-drop event (`onDragDropEvent`) plus an `"Attach Files"` button using `@tauri-apps/plugin-dialog`.
  - **Image Files (`png`, `jpg`, `jpeg`, `gif`, `webp`, `bmp`)**: Read via a Rust command `read_file_attachment(path)` (or `FileReader` for browser drops) into `{ type: "image", data: base64, mimeType }` and pass in the `images` array of Pi RPC `prompt` / `steer` / `follow_up`. Show image thumbnails above the input.
  - **Code / Text / Other Files**: Render removable file chips above the input showing filename and relative path; when submitting the prompt, prepend `@<relative_or_abs_path>` references (or inline fenced content if dropped from outside the workspace) so Pi's file tools have immediate context.
