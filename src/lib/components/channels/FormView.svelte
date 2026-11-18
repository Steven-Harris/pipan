<script lang="ts">
  // Renders a `form` request: optional markdown body, fields, and action buttons. Calls
  // `onanswer` with the answer once an action passes validation.
  import {
    buildAnswer,
    setFieldValue,
    appliesTo,
    defaultAction,
    initialValues,
    needsValidation,
    validateValues,
    type FormErrors,
    type FormValues
  } from '#lib/channels/form-state.ts';
  import type { FormAction, FormAnswer, FormField, FormRequest } from '#lib/channels/types.ts';
  import Markdown from './Markdown.svelte';

  let { form, onanswer }: { form: FormRequest; onanswer: (answer: FormAnswer) => void } = $props();

  // The host remounts this view for every request, so reading the prop once is intended.
  // svelte-ignore state_referenced_locally
  let values = $state<FormValues>(initialValues(form));
  let errors = $state<FormErrors>({});
  /** The action whose validation last ran. Later edits revalidate against it. */
  let attemptedAction = $state<string | null>(null);

  const fields = $derived(form.fields ?? []);
  /** Native select beyond this many options. */
  const RADIO_LIMIT = 12;

  function revalidate() {
    if (attemptedAction !== null) errors = validateValues(form, values, attemptedAction);
  }

  function setText(field: FormField, value: string) {
    values = setFieldValue(form, values, field.id, value);
    revalidate();
  }

  function toggle(field: FormField, option: string, checked: boolean) {
    const current = Array.isArray(values[field.id]) ? (values[field.id] as string[]) : [];
    values[field.id] = checked ? [...current.filter((v) => v !== option), option] : current.filter((v) => v !== option);
    revalidate();
  }

  function press(action: FormAction) {
    if (needsValidation(action)) {
      attemptedAction = action.id;
      errors = validateValues(form, values, action.id);
      if (Object.keys(errors).length > 0) {
        const firstId = fields.find((f) => errors[f.id])?.id;
        if (firstId) document.getElementById(`field-${firstId}`)?.focus();
        return;
      }
    }
    onanswer(buildAnswer(form, action.id, values));
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    const action = defaultAction(form);
    if (action) press(action);
  }

  /** The action that decides which requirements show: the last one pressed, else the Enter action. */
  const currentActionId = $derived(attemptedAction ?? defaultAction(form)?.id);
  /** True when the field's requirement applies to the current action. */
  const applies = (field: FormField): boolean =>
    currentActionId === undefined ? field.requiredFor === undefined : appliesTo(field, currentActionId);
  /** Marks a select or text field as required while the current action needs it. */
  const marked = (field: FormField): boolean => field.type !== 'multiselect' && field.required === true && applies(field);

  const actionClass = (style: FormAction['style']) =>
    style === 'primary'
      ? 'border-indigo-500 bg-indigo-600 text-white hover:bg-indigo-500'
      : style === 'danger'
        ? 'border-red-600/70 bg-red-900/40 text-red-100 hover:bg-red-800/60'
        : 'border-zinc-700 bg-zinc-800 text-zinc-200 hover:bg-zinc-700';

  // The first action that is not destructive receives initial focus when there is no field.
  const focusActionId = $derived(form.actions.find((a) => a.style !== 'danger')?.id ?? form.actions[0]?.id);
</script>

<form onsubmit={submit} class="flex min-h-0 flex-auto flex-col" novalidate>
  <div class="min-h-0 flex-auto space-y-4 overflow-y-auto px-4 py-3 text-xs text-zinc-300">
    {#if form.body}
      <div class="max-h-60 overflow-y-auto"><Markdown source={form.body} /></div>
    {/if}

    {#each fields as field, index (field.id)}
      {@const error = errors[field.id]}
      {@const errorId = `field-${field.id}-error`}
      {#if field.type === 'select' && field.options.length <= RADIO_LIMIT}
        <fieldset class="max-h-72 space-y-1.5 overflow-y-auto" aria-describedby={error ? errorId : undefined}>
          <legend class="mb-1 font-medium text-zinc-100">
            {field.label}{#if marked(field)}<span class="text-red-400" aria-hidden="true"> *</span>{/if}
          </legend>
          {#each field.options as option, i (option.value)}
            <label class="flex cursor-pointer items-start gap-2 rounded-md px-1 py-0.5 hover:bg-zinc-800/60">
              <input
                id={i === 0 ? `field-${field.id}` : undefined}
                type="radio"
                name={field.id}
                value={option.value}
                checked={values[field.id] === option.value}
                data-autofocus={(index === 0 && i === 0) || undefined}
                onchange={() => {
                  values = setFieldValue(form, values, field.id, option.value);
                  revalidate();
                }}
                class="mt-0.5 accent-indigo-500"
              />
              <span>
                <span class="text-zinc-100">{option.label}</span>
                {#if option.description}<span class="block text-[11px] text-zinc-500">{option.description}</span>{/if}
              </span>
            </label>
          {/each}
        </fieldset>
      {:else if field.type === 'select'}
        <div>
          <label for="field-{field.id}" class="mb-1 block font-medium text-zinc-100">
            {field.label}{#if marked(field)}<span class="text-red-400" aria-hidden="true"> *</span>{/if}
          </label>
          <select
            id="field-{field.id}"
            value={values[field.id]}
            data-autofocus={(index === 0) || undefined}
            aria-invalid={error ? 'true' : undefined}
            aria-describedby={error ? errorId : undefined}
            onchange={(e) => {
              values = setFieldValue(form, values, field.id, e.currentTarget.value);
              revalidate();
            }}
            class="w-full rounded-md border border-zinc-700 bg-zinc-950 px-2 py-1.5 text-zinc-100"
          >
            <option value="">Select...</option>
            {#each field.options as option (option.value)}
              <option value={option.value}>{option.label}</option>
            {/each}
          </select>
        </div>
      {:else if field.type === 'multiselect'}
        {@const min = applies(field) ? field.min : undefined}
        <fieldset class="space-y-1.5" aria-describedby={error ? errorId : undefined}>
          <legend class="mb-1 font-medium text-zinc-100">
            {field.label}
            {#if min !== undefined || field.max !== undefined}
              <span class="font-normal text-zinc-500">
                ({min !== undefined ? `min ${min}` : ''}{min !== undefined && field.max !== undefined
                  ? ', '
                  : ''}{field.max !== undefined ? `max ${field.max}` : ''})
              </span>
            {/if}
          </legend>
          {#each field.options as option, i (option.value)}
            <label class="flex cursor-pointer items-start gap-2 rounded-md px-1 py-0.5 hover:bg-zinc-800/60">
              <input
                id={i === 0 ? `field-${field.id}` : undefined}
                type="checkbox"
                value={option.value}
                checked={Array.isArray(values[field.id]) && (values[field.id] as string[]).includes(option.value)}
                data-autofocus={(index === 0 && i === 0) || undefined}
                aria-invalid={error ? 'true' : undefined}
                onchange={(e) => toggle(field, option.value, e.currentTarget.checked)}
                class="mt-0.5 accent-indigo-500"
              />
              <span>
                <span class="text-zinc-100">{option.label}</span>
                {#if option.description}<span class="block text-[11px] text-zinc-500">{option.description}</span>{/if}
              </span>
            </label>
          {/each}
        </fieldset>
      {:else if field.type === 'text'}
        <div>
          <label for="field-{field.id}" class="mb-1 block font-medium text-zinc-100">
            {field.label}{#if marked(field)}<span class="text-red-400" aria-hidden="true"> *</span>{/if}
          </label>
          {#if field.multiline}
            <textarea
              id="field-{field.id}"
              rows="6"
              value={values[field.id] as string}
              placeholder={field.placeholder}
              data-autofocus={(index === 0) || undefined}
              aria-invalid={error ? 'true' : undefined}
              aria-describedby={error ? errorId : undefined}
              oninput={(e) => setText(field, e.currentTarget.value)}
              class="w-full resize-y rounded-md border border-zinc-700 bg-zinc-950 px-2 py-1.5 font-mono text-zinc-100"
            ></textarea>
          {:else}
            <input
              id="field-{field.id}"
              type="text"
              value={values[field.id] as string}
              placeholder={field.placeholder}
              data-autofocus={(index === 0) || undefined}
              aria-invalid={error ? 'true' : undefined}
              aria-describedby={error ? errorId : undefined}
              oninput={(e) => setText(field, e.currentTarget.value)}
              class="w-full rounded-md border border-zinc-700 bg-zinc-950 px-2 py-1.5 text-zinc-100"
            />
          {/if}
        </div>
      {/if}
      {#if error}
        <p id={errorId} class="-mt-2 text-[11px] text-red-400">{error}</p>
      {/if}
    {/each}
  </div>

  <div class="flex shrink-0 flex-wrap items-center justify-end gap-2 border-t border-zinc-800 px-4 py-3">
    {#each form.actions as action (action.id)}
      <button
        type="button"
        onclick={() => press(action)}
        title={action.description}
        data-autofocus={(fields.length === 0 && action.id === focusActionId) || undefined}
        class="rounded-md border px-3 py-1.5 text-xs font-medium transition cursor-pointer {actionClass(action.style)}"
        >{action.label}</button
      >
    {/each}
  </div>
</form>
