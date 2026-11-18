<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';

  export let label: string;
  export let closeDisabled = false;
  export let palette = false;

  const dispatch = createEventDispatcher<{ close: void }>();
  let dialog: HTMLDialogElement;
  let composing = false;

  onMount(() => {
    dialog.showModal();
    dialog.querySelector<HTMLElement>('[data-initial-focus]')?.focus();
  });

  function requestClose() {
    if (!closeDisabled && !composing) dispatch('close');
  }

  function handleEscape(event: KeyboardEvent) {
    if (event.key !== 'Escape' || event.isComposing || event.keyCode === 229 || composing) return;
    event.preventDefault();
    requestClose();
  }
</script>

<!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
<dialog
  class="modal-card"
  class:palette-card={palette}
  aria-label={label}
  bind:this={dialog}
  on:cancel|preventDefault={requestClose}
  on:compositionstart={() => (composing = true)}
  on:compositionend={() => (composing = false)}
  on:keydown={handleEscape}
  on:keydown
  on:click={(event) => {
    if (event.target === dialog) requestClose();
  }}
>
  <div class="modal-content"><slot /></div>
</dialog>
