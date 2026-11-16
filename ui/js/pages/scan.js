// The Scan page: read cards with the camera and add them.

import { api, gameAPI } from "../api.js";
import { addCard, addDefaults, pickList } from "../cards.js";
import { $, fill, h, status } from "../dom.js";
import { fname, gameInfo, sn } from "../format.js";
import { state } from "../state.js";

// Scan: point the camera at a card (or choose a photo); the text on it identifies the
// printing. The camera captures once two frames in a row read the same.
export function viewScan() {
  if (!gameInfo().Scan) {
    fill($("#main"), h("h1", "Scan a Card"), h("p", "Card scanning reads the text on a card with Apple's Vision framework, so it works in the macOS and iPhone apps (and the web app when it runs on a Mac)."),
      h("p", h("a.button", { href: "#/add" }, "Search for a card instead")));
    return;
  }
  const say = h("div.say");
  const video = h("video", { playsinline: true, muted: true, autoplay: true });
  const camera = h("div.camera", { hidden: true }, video, h("div.guide"), say);
  const result = h("div");
  const file = h("input", { type: "file", accept: "image/*", hidden: true, onchange: () => {
    if (file.files.length) identify(file.files[0]);
    file.value = "";
  } });
  const startBtn = h("button.primary", { type: "button", onclick: () => startCamera() }, "Use the camera");
  const photoBtn = h("button", { type: "button", onclick: () => file.click() }, "Choose a photo…");
  const canCamera = !!(navigator.mediaDevices && navigator.mediaDevices.getUserMedia);

  const identify = async (blob) => {
    stopCamera();
    fill(result, h("p.muted.scan-read", "Reading the card…"));
    try {
      const r = await api("POST", gameAPI("/scan"), blob);
      const preview = h("div");
      // After adding a scanned card, go straight back to the camera for the next one.
      const again = () => { if (canCamera) startCamera(); else fill(result); };
      const picker = pickList(r.candidates, r.owned, preview, again);
      const best = r.candidates[0];
      let quick = null;
      if (r.exact && best) {
        const d = addDefaults();
        const fin = (best.finishes || []).includes(d.finish) ? d.finish : (best.finishes || [])[0] || "";
        quick = h("button.primary", { type: "button", onclick: async () => {
          quick.disabled = true;
          try {
            await addCard(best, { quantity: 1, finish: fin, condition: d.condition, price: d.price, date: d.date });
            status(`Added 1× ${best.name} (${sn(best)})`);
            again();
          } catch (e) { status(e.message, true); quick.disabled = false; }
        } }, `Add 1× ${best.name} (${sn(best)}) · ${fname(fin)} · ${d.condition}`);
      }
      fill(result,
        h("p.scan-read", "Read: ", h("strong", r.read)),
        r.note ? h("p.note.scan-read", r.note) : null,
        quick ? h("p.scan-read", quick, " ", h("span.muted", "or set the details below")) : null,
        r.candidates.length ? h("div.split",
          h("div.pane-list", h("div.list.results", picker.items)), preview)
          : h("p.scan-read", h("a.button", { href: `#/add?${new URLSearchParams({ text: r.name || "" })}` }, "Search by hand")));
      if (r.candidates.length) picker.select(0);
    } catch (e) { fill(result, h("p.error.scan-read", e.message)); }
  };

  let stream = null, timer = null;
  const stopCamera = () => {
    clearTimeout(timer);
    if (stream) stream.getTracks().forEach((t) => t.stop());
    stream = null;
    camera.hidden = true;
    startBtn.hidden = !canCamera;
  };
  state.scanStop = stopCamera;
  // frame grabs the camera's picture, at most 1920 pixels across: plenty for the
  // collector text, and far quicker to read than a 4K frame.
  const frame = () => new Promise((resolve) => {
    const scale = Math.min(1, 1920 / Math.max(video.videoWidth, video.videoHeight));
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(video.videoWidth * scale);
    canvas.height = Math.round(video.videoHeight * scale);
    canvas.getContext("2d").drawImage(video, 0, 0, canvas.width, canvas.height);
    canvas.toBlob(resolve, "image/jpeg", 0.9);
  });
  const startCamera = async () => {
    fill(result);
    try {
      stream = await navigator.mediaDevices.getUserMedia({ video: { facingMode: "environment", width: { ideal: 1920 }, height: { ideal: 1440 } }, audio: false });
    } catch (e) {
      status(`The camera isn't available (${e.message}); choose a photo instead.`, true);
      return;
    }
    video.srcObject = stream;
    camera.hidden = false;
    startBtn.hidden = true;
    say.textContent = "Fill the frame with the card";
    let last = "";
    const tick = async () => {
      if (!stream) return;
      if (video.videoWidth) {
        const blob = await frame();
        try {
          const r = await api("POST", gameAPI("/scan/frame"), blob);
          if (!stream) return;
          if (r.complete && r.key === last) { identify(blob); return; }
          last = r.complete ? r.key : "";
          say.textContent = r.complete ? "Hold still…" : r.lines ? "Getting closer — keep the card flat and in the light" : "Fill the frame with the card";
        } catch (e) { say.textContent = e.message; }
      }
      timer = setTimeout(tick, 350);
    };
    tick();
  };

  fill($("#main"), h("h1", "Scan a Card"),
    h("p.muted", "Hold the card flat with its name and the bottom-left corner (set and collector number) in view. Cards from before 2014 are matched by name, so you pick the printing."),
    camera, h("div.scan-actions", canCamera ? startBtn : null, photoBtn, file), result);
  if (canCamera && gameInfo().Scan && state.info.client === "phone") startCamera();
}
