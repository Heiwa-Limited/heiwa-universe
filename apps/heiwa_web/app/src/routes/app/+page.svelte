<script lang="ts">
  import type { PageData } from "./$types";

  export let data: PageData;
</script>

<svelte:head>
  <title>Heiwa</title>
</svelte:head>

<main>
  {#if data.authFailed}
    <p>Sign-in failed. Try again.</p>
  {/if}

  <h1>Heiwa</h1>
</main>
