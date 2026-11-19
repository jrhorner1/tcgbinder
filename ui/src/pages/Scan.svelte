<!-- Scanning cards: point the camera at a card (or choose a photo); the text on it
     identifies the printing. The camera captures once two frames in a row agree. -->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, app, failed, gameAPI, status } from "../lib/app.svelte";
  import { addCard, addDefaults, gameInfo } from "../lib/cards";
  import { fname, sn } from "../lib/format";
  import type { CatalogCard } from "../lib/types";
  import { openAdd } from "../add/add";
  import CardSearch, { type Picked } from "../add/CardSearch.svelte";

  interface ScanResult { read: string; name: string; exact: boolean; note: string; candidates: CatalogCard[]; owned: number[] }

  const canCamera = !!navigator.mediaDevices?.getUserMedia;
  let video = $state<HTMLVideoElement>();
  let stream: MediaStream | null = null;
  let watching = $state(false);
  let say = $state("");
  let reading = $state(false);
  let result = $state<ScanResult | null>(null);
  let error = $state("");
  let timer: ReturnType<typeof setTimeout>;
  let file = $state() as HTMLInputElement;

  function stop() {
    clearTimeout(timer);
    stream?.getTracks().forEach((t) => t.stop());
    stream = null;
    watching = false;
  }
  onDestroy(stop);

  async function identify(blob: Blob) {
    stop();
    reading = true; result = null; error = "";
    try { result = await api<ScanResult>("POST", gameAPI("/scan"), blob); } catch (e) { error = e instanceof Error ? e.message : String(e); }
    reading = false;
  }

  // The camera's picture, at most 1920 pixels across: plenty for the collector text,
  // and far quicker to read than a 4K frame.
  const frame = () => new Promise<Blob>((resolve) => {
    const v = video!;
    const scale = Math.min(1, 1920 / Math.max(v.videoWidth, v.videoHeight));
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(v.videoWidth * scale);
    canvas.height = Math.round(v.videoHeight * scale);
    canvas.getContext("2d")!.drawImage(v, 0, 0, canvas.width, canvas.height);
    canvas.toBlob((b) => resolve(b!), "image/jpeg", 0.9);
  });

  async function start() {
    result = null; error = "";
    try {
      stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment", width: { ideal: 1920 }, height: { ideal: 1440 } }, audio: false });
    } catch (e) {
      status(`The camera isn't available (${e instanceof Error ? e.message : e}); choose a photo instead.`, true);
      return;
    }
    watching = true;
    await Promise.resolve();
    video!.srcObject = stream;
    say = "Fill the frame with the card";
    let last = "";
    const tick = async () => {
      if (!stream) return;
      if (video?.videoWidth) {
        const blob = await frame();
        try {
          const r = await api<{ key: string; complete: boolean; lines: number }>("POST", gameAPI("/scan/frame"), blob);
          if (!stream) return;
          if (r.complete && r.key === last) { identify(blob); return; }
          last = r.complete ? r.key : "";
          say = r.complete ? "Hold still…" : r.lines ? "Getting closer — keep the card flat and in the light" : "Fill the frame with the card";
        } catch (e) { say = e instanceof Error ? e.message : String(e); }
      }
      timer = setTimeout(tick, 350);
    };
    tick();
  }
  // After adding a scanned card, straight back to the camera for the next one.
  const again = () => { result = null; if (canCamera) start(); };

  async function quickAdd(best: CatalogCard) {
    const d = addDefaults();
    const fin = best.finishes.includes(d.finish || "") ? d.finish! : best.finishes[0] || "";
    try {
      await addCard(best, { quantity: 1, finish: fin, condition: d.condition || "NM", price: d.price || "", date: d.date || "", tags: d.tags });
      status(`Added 1× ${best.name} (${sn(best)})`);
      again();
    } catch (e) { failed(e); }
  }
  async function pick(c: CatalogCard, a: Picked) {
    const d = addDefaults();
    await addCard(c, { ...a, price: d.price || "", date: d.date || "", tags: d.tags });
    status(`Added ${a.quantity}× ${c.name} (${sn(c)})`);
    again();
  }

  onMount(() => { if (canCamera && gameInfo().Scan && app.info!.client === "phone") start(); });
</script>

<h1>Scan a Card</h1>
{#if !gameInfo().Scan}
  <p>Card scanning reads the text on a card with Apple's Vision framework, so it works in the macOS and iPhone apps (and the web app when it runs on a Mac).</p>
  <p><button type="button" onclick={() => openAdd()}>Search for a card instead</button></p>
{:else}
  <p class="muted">Hold the card flat with its name and the bottom-left corner (set and collector number) in view. Cards from before 2014 are matched by name, so you pick the printing.</p>
  {#if watching}
    <div class="camera">
      <video bind:this={video} playsinline muted autoplay></video>
      <div class="guide"></div>
      <div class="say">{say}</div>
    </div>
  {/if}
  <div class="scan-actions">
    {#if canCamera && !watching}<button type="button" class="primary" onclick={start}>Use the camera</button>{/if}
    <button type="button" onclick={() => file.click()}>Choose a photo…</button>
    <input bind:this={file} type="file" accept="image/*" hidden onchange={() => { if (file.files?.length) identify(file.files[0]); file.value = ""; }} />
  </div>
  {#if reading}<p class="muted scan-read">Reading the card…</p>{/if}
  {#if error}<p class="error scan-read">{error}</p>{/if}
  {#if result}
    {@const best = result.candidates[0]}
    <p class="scan-read">Read: <strong>{result.read}</strong></p>
    {#if result.note}<p class="note scan-read">{result.note}</p>{/if}
    {#if result.exact && best}
      {@const d = addDefaults()}
      <p class="scan-read">
        <button type="button" class="primary" onclick={() => quickAdd(best)}>Add 1× {best.name} ({sn(best)}) · {fname(best.finishes.includes(d.finish || "") ? d.finish! : best.finishes[0] || "")} · {d.condition}</button>
        <span class="muted">or choose below</span>
      </p>
    {/if}
    {#if result.candidates.length}
      <CardSearch query={result.name} found={{ cards: result.candidates, owned: result.owned }} onadd={pick} />
    {:else}
      <p class="scan-read"><button type="button" onclick={() => openAdd({ query: result!.name || "" })}>Search by hand</button></p>
    {/if}
  {/if}
{/if}

<style>
  .camera { position: relative; background: #000; border-radius: var(--radius); overflow: hidden; aspect-ratio: 3 / 4; max-height: 70vh; margin: 0 auto 12px; width: 100%; max-width: 520px; }
  .camera video { width: 100%; height: 100%; object-fit: cover; display: block; }
  /* The frame to fill with the card. */
  .guide { position: absolute; inset: 8% 14%; border: 3px solid rgba(255, 255, 255, .85); border-radius: 18px; box-shadow: 0 0 0 999px rgba(0, 0, 0, .35); pointer-events: none; }
  .say { position: absolute; left: 0; right: 0; bottom: 14px; text-align: center; color: #fff; font-weight: 600; text-shadow: 0 1px 3px #000; }
  .scan-actions { display: flex; gap: 8px; justify-content: center; flex-wrap: wrap; margin-bottom: 12px; }
  .scan-read { text-align: center; }
</style>
