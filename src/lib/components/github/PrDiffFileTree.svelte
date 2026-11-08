<script lang="ts">
	import {
		FolderTree,
		ChevronRight,
		ChevronDown,
		FileCode,
		Plus,
		Minus
	} from 'lucide-svelte';
	import type { PrDiffFile } from '#lib/types/github.ts';

	interface TreeNode {
		name: string;
		path: string;
		isDir: boolean;
		file?: PrDiffFile;
		children: Record<string, TreeNode>;
		additions: number;
		deletions: number;
	}

	let {
		files,
		selectedPath = null,
		onSelectFile
	}: {
		files: PrDiffFile[];
		selectedPath?: string | null;
		onSelectFile: (path: string | null) => void;
	} = $props();

	let collapsedDirs = $state<Record<string, boolean>>({});

	function toggleDir(dirPath: string) {
		collapsedDirs[dirPath] = !collapsedDirs[dirPath];
	}

	const tree = $derived.by((): TreeNode => {
		const root: TreeNode = {
			name: '',
			path: '',
			isDir: true,
			children: {},
			additions: 0,
			deletions: 0
		};

		for (const file of files) {
			const parts = file.path.split('/').filter(Boolean);
			let curr = root;
			curr.additions += file.additions;
			curr.deletions += file.deletions;

			let currPath = '';
			for (let i = 0; i < parts.length; i++) {
				const part = parts[i];
				currPath = currPath ? `${currPath}/${part}` : part;
				const isLast = i === parts.length - 1;

				if (!curr.children[part]) {
					curr.children[part] = {
						name: part,
						path: currPath,
						isDir: !isLast,
						file: isLast ? file : undefined,
						children: {},
						additions: 0,
						deletions: 0
					};
				}
				curr = curr.children[part];
				if (!isLast) {
					curr.additions += file.additions;
					curr.deletions += file.deletions;
				}
			}
		}

		return root;
	});

	function sortedChildren(node: TreeNode): TreeNode[] {
		return Object.values(node.children).sort((a, b) => {
			if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
			return a.name.localeCompare(b.name);
		});
	}

	function fileStatusBadge(status: string) {
		switch (status) {
			case 'added':
				return { label: 'A', cls: 'text-emerald-400 bg-emerald-950/40 border-emerald-500/30' };
			case 'deleted':
				return { label: 'D', cls: 'text-rose-400 bg-rose-950/40 border-rose-500/30' };
			case 'renamed':
				return { label: 'R', cls: 'text-sky-400 bg-sky-950/40 border-sky-500/30' };
			default:
				return { label: 'M', cls: 'text-amber-400 bg-amber-950/40 border-amber-500/30' };
		}
	}
</script>

{#snippet renderNode(node: TreeNode, depth: number)}
	{@const isCollapsed = Boolean(collapsedDirs[node.path])}
	{@const isSelected = selectedPath === node.path}

	{#if node.isDir}
		<div>
			<button
				type="button"
				onclick={() => toggleDir(node.path)}
				style:padding-left="{depth * 12 + 8}px"
				class="flex w-full items-center justify-between gap-1 py-1 pr-2 text-left text-xs text-zinc-300 hover:bg-zinc-800/60 hover:text-zinc-100 cursor-pointer"
			>
				<div class="flex min-w-0 items-center gap-1.5">
					{#if isCollapsed}
						<ChevronRight class="h-3 w-3 shrink-0 text-zinc-500" />
					{:else}
						<ChevronDown class="h-3 w-3 shrink-0 text-zinc-500" />
					{/if}
					<span class="truncate font-mono text-[11px] font-medium text-zinc-200">{node.name}/</span>
				</div>
				<span class="shrink-0 font-mono text-[10px] text-zinc-500">
					+{node.additions} -{node.deletions}
				</span>
			</button>

			{#if !isCollapsed}
				{#each sortedChildren(node) as child (child.path)}
					{@render renderNode(child, depth + 1)}
				{/each}
			{/if}
		</div>
	{:else}
		{@const file = node.file!}
		{@const badge = fileStatusBadge(file.status)}
		<button
			type="button"
			onclick={() => onSelectFile(isSelected ? null : file.path)}
			style:padding-left="{depth * 12 + 18}px"
			class="flex w-full items-center justify-between gap-1 py-1 pr-2 text-left text-xs transition cursor-pointer {isSelected
				? 'bg-indigo-600/20 text-indigo-100 border-l-2 border-indigo-400 font-medium'
				: 'text-zinc-300 hover:bg-zinc-900 hover:text-zinc-100 border-l-2 border-transparent'}"
			title={file.path}
		>
			<div class="flex min-w-0 items-center gap-1.5">
				<FileCode class="h-3 w-3 shrink-0 {isSelected ? 'text-indigo-400' : 'text-zinc-500'}" />
				<span class="truncate font-mono text-[11px]">{node.name}</span>
				<span class="rounded border px-1 text-[9px] font-mono {badge.cls}">{badge.label}</span>
			</div>
			<div class="shrink-0 font-mono text-[10px] space-x-1">
				<span class="text-emerald-400">+{file.additions}</span>
				<span class="text-rose-400">-{file.deletions}</span>
			</div>
		</button>
	{/if}
{/snippet}

<div class="flex h-full min-h-0 flex-col overflow-hidden bg-zinc-950 text-zinc-200 select-none">
	<div class="flex shrink-0 items-center justify-between border-b border-zinc-800/80 bg-zinc-900/60 px-3 py-1.5 text-xs">
		<div class="flex items-center gap-1.5 font-semibold text-zinc-200">
			<FolderTree class="h-3.5 w-3.5 text-indigo-400" />
			<span>Files Changed</span>
			<span class="rounded-full bg-zinc-800 px-1.5 py-0.2 text-[10px] text-zinc-400 font-mono">
				{files.length}
			</span>
		</div>
		{#if selectedPath}
			<button
				type="button"
				onclick={() => onSelectFile(null)}
				class="text-[11px] text-indigo-400 hover:underline cursor-pointer"
			>
				Show all
			</button>
		{/if}
	</div>

	<div class="min-h-0 flex-1 overflow-y-auto py-1">
		{#if files.length === 0}
			<div class="p-4 text-center text-xs text-zinc-500">No changed files</div>
		{:else}
			{#each sortedChildren(tree) as child (child.path)}
				{@render renderNode(child, 0)}
			{/each}
		{/if}
	</div>
</div>
