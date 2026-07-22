// murmr recording pill — driven by Tauri events from the backend.
// States: recording (dot + waveform + running timer) → processing (spinner).
// The backend shows/hides the window; this script only manages the visual
// state and the elapsed timer.

const pill = document.getElementById('pill');
const timerEl = document.getElementById('timer');

let startMs = null;
let rafId = null;
let processing = false;

function fmt(ms) {
  const s = Math.floor(ms / 1000);
  const m = Math.floor(s / 60);
  return `${m}:${String(s % 60).padStart(2, '0')}`;
}

function tick() {
  if (startMs == null) return;
  timerEl.textContent = fmt(Date.now() - startMs);
  rafId = requestAnimationFrame(tick);
}

function startRecording() {
  pill.className = 'pill state-recording';
  processing = false;
  startMs = Date.now();
  timerEl.textContent = '0:00';
  cancelAnimationFrame(rafId);
  tick();
}

function startProcessing() {
  pill.className = 'pill state-processing';
  processing = true;
  // freeze the timer at its final recorded value
  cancelAnimationFrame(rafId);
}

const labelEl = document.getElementById('label');

function showDone() {
  processing = true;
  cancelAnimationFrame(rafId);
  pill.className = 'pill state-done';
  if (labelEl) labelEl.textContent = 'Pasted';
}

function showPreview(text) {
  processing = true;
  cancelAnimationFrame(rafId);
  pill.className = 'pill state-preview';
  // The text is already on the clipboard — tell the user to paste it. (Ellipsis
  // handled by CSS; keep a slightly longer prefix so the pill feels informative.)
  const clean = (text || '').replace(/\s+/g, ' ').trim();
  const preview = clean.slice(0, 22);
  if (labelEl) {
    labelEl.textContent = clean ? `⌘V  “${preview}${clean.length > 22 ? '…' : ''}”` : '⌘V to paste';
  }
}

function showError(msg) {
  processing = true; // keep visibility handler from resetting to recording
  cancelAnimationFrame(rafId);
  pill.className = 'pill state-error';
  if (labelEl) labelEl.textContent = msg || 'Something went wrong';
}

// Wire to Tauri events if present (absent when previewed in a plain browser).
// The backend emits pill-specific events (pill:record / pill:process) so the
// pill's own state changes can never feed back into the recording lifecycle.
if (window.__TAURI__ && window.__TAURI__.event) {
  const { listen } = window.__TAURI__.event;
  listen('pill:record', startRecording);
  listen('pill:process', startProcessing);
  listen('pill:done', showDone);
  listen('pill:preview', (e) => showPreview(e.payload));
  listen('pill:error', (e) => showError(e.payload));

  // Every state is driven explicitly by the backend events above (the backend
  // emits pill:record whenever it shows the pill to record). We deliberately do
  // NOT reset state on visibilitychange — that raced with pill:process /
  // pill:preview and could revert the pill to the recording state, hiding the
  // preview text.
  timerEl.textContent = '0:00';
} else {
  // No Tauri bridge (e.g. opened in a plain browser for preview): just animate.
  startRecording();
}
