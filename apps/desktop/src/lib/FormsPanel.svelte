<script lang="ts">
  // Renders `list_form_fields_cmd`'s output as an editable panel. Widgets
  // are grouped by field `name` — a Checkbox/RadioButton name can appear
  // more than once (one entry per widget in its control group; see
  // openpdfedit-engine's `FormField` doc) — so a radio group renders as
  // one radio-button row per option, not one row per raw entry.
  //
  // Every edit fires `onFill` with a single-entry `{name: value}` map
  // immediately (on blur/Enter for text, on change for checkbox/radio) —
  // no separate "Apply" step, matching PagesPanel's "click applies
  // immediately" pattern. Each fill rotates the document's handle (see
  // `forms.rs`'s module doc), so the parent is expected to re-fetch this
  // panel's `fields` prop after every `onFill` call.

  import type { FormFieldDto } from "./backend/types";

  interface Props {
    fields: FormFieldDto[];
    busy: boolean;
    onFill: (values: Record<string, string>) => void;
  }

  let { fields, busy, onFill }: Props = $props();

  const grouped = $derived.by(() => {
    const byName = new Map<string, FormFieldDto[]>();
    for (const f of fields) {
      const list = byName.get(f.name) ?? [];
      list.push(f);
      byName.set(f.name, list);
    }
    return [...byName.entries()];
  });

  // Local in-progress text so typing doesn't get clobbered by a refetch
  // mid-edit; cleared once the field's own committed value catches up.
  let textDrafts = $state<Record<string, string>>({});

  function textValue(name: string, committed: string | null): string {
    return textDrafts[name] ?? committed ?? "";
  }

  function commitText(name: string) {
    const draft = textDrafts[name];
    if (draft === undefined) return;
    onFill({ [name]: draft });
    delete textDrafts[name];
  }
</script>

<aside class="oa-panel">
  <div class="oa-panel__header">
    <span class="oa-panel__title">Form fields</span>
  </div>
  <div class="oa-panel__body">
    {#if grouped.length === 0}
      <p class="oa-empty">This document has no fillable form fields.</p>
    {:else}
      <ul class="oa-list">
        {#each grouped as [name, widgets] (name)}
          <li class="oa-list-item">
            <span class="field-name">{name}</span>

            {#if widgets[0].kind === "text"}
              <input
                class="oa-input"
                type="text"
                value={textValue(name, widgets[0].value)}
                disabled={busy || widgets[0].isReadOnly}
                oninput={(e) => (textDrafts[name] = (e.target as HTMLInputElement).value)}
                onblur={() => commitText(name)}
                onkeydown={(e) => e.key === "Enter" && commitText(name)}
              />
            {:else if widgets[0].kind === "checkbox"}
              <label class="checkbox-row">
                <input
                  class="oa-checkbox"
                  type="checkbox"
                  checked={widgets[0].isChecked ?? false}
                  disabled={busy || widgets[0].isReadOnly}
                  onchange={(e) => onFill({ [name]: (e.target as HTMLInputElement).checked ? "true" : "false" })}
                />
              </label>
            {:else if widgets[0].kind === "radioButton"}
              <div class="radio-group">
                <span class="oa-tag" title="Selection is not guaranteed to persist reliably yet — see PLAN.md">experimental</span>
                {#each widgets as w (w.value ?? w.pageIndex)}
                  <label class="radio-option">
                    <input
                      class="oa-radio"
                      type="radio"
                      name={`form-radio-${name}`}
                      checked={w.isChecked ?? false}
                      disabled={busy || w.isReadOnly || !w.value}
                      onchange={() => w.value && onFill({ [name]: w.value })}
                    />
                    {w.value ?? "(unnamed option)"}
                  </label>
                {/each}
              </div>
            {:else if widgets[0].kind === "comboBox" || widgets[0].kind === "listBox"}
              <span class="unsupported">{widgets[0].value ?? "(no selection)"} — editing not supported yet</span>
            {:else}
              <span class="unsupported">not fillable</span>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</aside>

<style>
  .field-name {
    font: var(--type-ui);
    color: var(--text-strong);
    word-break: break-word;
  }

  .checkbox-row,
  .radio-option {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font: var(--type-body);
    font-size: var(--text-sm);
    color: var(--text-body);
  }

  .radio-group {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
  }

  .unsupported {
    font: var(--type-caption);
    font-style: italic;
  }
</style>
