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
  listen('pill:error', (e) => showError(e.payload));

  // The pill window is persistent (shown/hidden by the backend, never
  // recreated), so we must NOT start the clock at load — that would count from
  // app launch. Instead, reset to zero every time the window actually becomes
  // visible. This fires exactly when the backend shows the pill to record,
  // independent of event-delivery timing, guaranteeing the timer starts at 0.
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible' && !processing) startRecording();
  });
  timerEl.textContent = '0:00';
} else {
  // Preview mode: just animate.
  startRecording();
}
