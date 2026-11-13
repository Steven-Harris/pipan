// Generates the JSON Schema of the channel payloads from src/lib/channels/types.ts.
//
//   pnpm channels:schema           write docs/public/channels.schema.json
//   pnpm channels:schema --check   fail when the checked-in file is out of date
//
// The schema describes shape only. The host also enforces limits and uniqueness rules that a
// schema cannot express; see docs/guide/channels.md.

import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createGenerator } from "ts-json-schema-generator";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const SCHEMA_PATH = resolve(root, "docs/public/channels.schema.json");

/** The schema as the text that is checked in. */
export function generateSchema() {
	const generator = createGenerator({
		path: resolve(root, "scripts/channels-schema-root.ts"),
		tsconfig: resolve(root, "scripts/tsconfig.channels-schema.json"),
		type: "ChannelPayloads",
		expose: "export",
		topRef: true,
		jsDoc: "extended",
		sortProps: true,
		// Hosts ignore unknown keys, so producers may send them.
		additionalProperties: true,
		skipTypeCheck: false,
	});
	const schema = generator.createSchema("ChannelPayloads");
	schema.title = "PiPan extension channel payloads";
	return `${JSON.stringify(schema, null, 2)}\n`;
}

async function main() {
	const text = generateSchema();
	if (process.argv.includes("--check")) {
		const current = await readFile(SCHEMA_PATH, "utf8").catch(() => "");
		if (current !== text) {
			console.error("docs/public/channels.schema.json is out of date. Run `pnpm channels:schema`.");
			process.exit(1);
		}
		console.log("channels.schema.json is up to date");
		return;
	}
	await mkdir(dirname(SCHEMA_PATH), { recursive: true });
	await writeFile(SCHEMA_PATH, text);
	console.log(`wrote ${SCHEMA_PATH}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();
