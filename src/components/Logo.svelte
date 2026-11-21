<script lang="ts">
  import full from "../../assets/logo/featherweight.svg?raw";
  import small from "../../assets/logo/featherweight-16.svg?raw";
  import mono from "../../assets/logo/featherweight-mono.svg?raw";

  interface Props {
    /** Rendered size in px. At 24 px and below the simplified 16 px mark is used. */
    size?: number;
    /** Single colour (currentColor) instead of the accent gradient. */
    monochrome?: boolean;
    /** Accessible name; leave empty when the mark sits next to the product name. */
    label?: string;
  }

  let { size = 24, monochrome = false, label = "" }: Props = $props();

  // Each instance gets its own gradient id so several marks can coexist.
  const uid = $props.id();

  const markup = $derived.by(() => {
    const source = monochrome ? mono : size <= 24 ? small : full;
    return source
      .replaceAll('id="fwg"', `id="fwg-${uid}"`)
      .replaceAll("url(#fwg)", `url(#fwg-${uid})`)
      .replace("<svg ", `<svg width="${size}" height="${size}" focusable="false" aria-hidden="true" `);
  });
</script>

<span
  class="logo"
  style:width="{size}px"
  style:height="{size}px"
  role={label ? "img" : undefined}
  aria-label={label || undefined}
  aria-hidden={label ? undefined : "true"}
>
  {@html markup}
</span>

<style>
  .logo {
    display: inline-flex;
    flex: none;
  }
  .logo :global(svg) {
    display: block;
  }
</style>
