const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// Tab switching — drives the segmented pill's sliding thumb via data-active
document.querySelectorAll('.tab').forEach(tab => {
  tab.addEventListener('click', () => {
    document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
    tab.classList.add('active');
    document.getElementById(tab.dataset.tab).classList.add('active');
    const nav = document.querySelector('nav.segmented');
    if (nav) nav.dataset.active = tab.dataset.tab;
  });
});

// Load initial state
async function init() {
  await loadConfig();
  await loadTranscripts();
  await loadDictionary();
  await loadModes();
  setupListeners();
  maybeShowOnboarding();
}

// Show the first-run permissions banner until the user dismisses it.
function maybeShowOnboarding() {
  if (localStorage.getItem('onboarding-dismissed') === '1') return;
  const el = document.getElementById('onboarding');
  if (el) el.hidden = false;
}

// The last config loaded from the backend. Save merges the form's fields into
// this so we never clobber settings the form doesn't expose (modes, stt,
// dictionary learning, etc.).
let currentConfig = null;

async function loadConfig() {
  try {
    const config = await invoke('get_config');
    currentConfig = config;
    document.getElementById('llm-protocol').value = config.llm.protocol || 'ollama';
    document.getElementById('llm-endpoint').value = config.llm.endpoint || '';
    document.getElementById('llm-apikey').value = config.llm.api_key || '';
    document.getElementById('llm-region').value = config.llm.region || '';
    const proto = config.llm.protocol || 'ollama';
    const spec = PROTOCOL_FIELDS[proto] || PROTOCOL_FIELDS.ollama;
    document.getElementById('llm-cleanup-model').value = config.llm.cleanup_model || spec.defaults.cleanup;
    document.getElementById('llm-command-model').value = config.llm.command_model || spec.defaults.command;
    document.getElementById('hotkey-dictate').value = config.hotkeys.dictate || '';
    document.getElementById('hotkey-command').value = config.hotkeys.command || '';
    document.getElementById('preview-before-paste').checked = !!(config.paste && config.paste.preview_before_paste);
    document.getElementById('codebase-path').value = (config.dictionary && config.dictionary.codebase_path) || '';
    // Mark the loaded protocol as active so applyProtocolFields keeps the saved
    // models instead of resetting them to defaults.
    activeProtocol = proto;
    applyProtocolFields();
  } catch (e) {
    console.error('failed to load config:', e);
  }
}

// Which LLM fields each protocol uses, its model suggestions, and defaults.
const PROTOCOL_FIELDS = {
  ollama: {
    endpoint: true, apikey: false, region: false,
    hint: 'Talks to a local Ollama server. No API key needed.',
    models: ['llama3.1', 'llama3.2', 'qwen2.5', 'mistral', 'gemma2', 'phi3'],
    defaults: { cleanup: 'llama3.1', command: 'llama3.1' },
  },
  openai: {
    endpoint: true, apikey: true, region: false,
    hint: 'Any OpenAI-compatible endpoint. Requires an API key.',
    models: ['gpt-4o', 'gpt-4o-mini', 'gpt-4.1', 'gpt-4.1-mini', 'o3-mini'],
    defaults: { cleanup: 'gpt-4o-mini', command: 'gpt-4o' },
  },
  anthropic: {
    endpoint: false, apikey: true, region: false,
    hint: 'Anthropic API. Requires an API key.',
    models: ['claude-haiku-4-5-20251001', 'claude-sonnet-4-20250514', 'claude-opus-4-1'],
    defaults: { cleanup: 'claude-haiku-4-5-20251001', command: 'claude-sonnet-4-20250514' },
  },
  bedrock: {
    endpoint: false, apikey: false, region: true,
    hint: 'AWS Bedrock uses your AWS credentials — no API key. Set the region.',
    models: [
      'us.anthropic.claude-haiku-4-5-20251001-v1:0',
      'us.anthropic.claude-sonnet-4-20250514-v1:0',
      'us.anthropic.claude-3-5-haiku-20241022-v1:0',
    ],
    defaults: {
      cleanup: 'us.anthropic.claude-haiku-4-5-20251001-v1:0',
      command: 'us.anthropic.claude-sonnet-4-20250514-v1:0',
    },
  },
};

// Per-provider model values, so switching provider swaps its remembered models.
const modelMemory = {};       // { protocol: { cleanup, command } }
let activeProtocol = null;    // protocol currently reflected in the model inputs

function currentProtocol() {
  return document.getElementById('llm-protocol').value || 'ollama';
}

// Show/hide LLM inputs, refresh model suggestions, and swap per-provider models.
function applyProtocolFields() {
  const proto = currentProtocol();
  const spec = PROTOCOL_FIELDS[proto] || PROTOCOL_FIELDS.ollama;

  document.getElementById('field-endpoint').style.display = spec.endpoint ? '' : 'none';
  document.getElementById('field-apikey').style.display   = spec.apikey   ? '' : 'none';
  document.getElementById('field-region').style.display   = spec.region   ? '' : 'none';
  document.getElementById('provider-hint').textContent = spec.hint;

  const cleanupEl = document.getElementById('llm-cleanup-model');
  const commandEl = document.getElementById('llm-command-model');

  // Stash the outgoing provider's model values before switching away.
  if (activeProtocol && activeProtocol !== proto) {
    modelMemory[activeProtocol] = {
      cleanup: cleanupEl.value,
      command: commandEl.value,
    };
  }

  // Restore this provider's remembered models, or fall back to its defaults.
  if (activeProtocol !== proto) {
    const remembered = modelMemory[proto];
    cleanupEl.value = remembered ? remembered.cleanup : spec.defaults.cleanup;
    commandEl.value = remembered ? remembered.command : spec.defaults.command;
    activeProtocol = proto;
  }

  // Refresh the shared datalist with this provider's suggestions.
  const dl = document.getElementById('model-suggestions');
  dl.innerHTML = spec.models.map(m => `<option value="${m}"></option>`).join('');
}

async function loadTranscripts() {
  try {
    const transcripts = await invoke('get_transcripts');
    renderTranscripts(transcripts);
  } catch (e) {
    console.error('failed to load transcripts:', e);
  }
}

// Turn an "HH:MM:SS" wall-clock stamp into a compact relative label.
function relativeTime(stamp) {
  const now = new Date();
  const [h, m, s] = String(stamp).split(':').map(Number);
  if ([h, m, s].some(Number.isNaN)) return stamp;
  const then = new Date(now);
  then.setHours(h, m, s, 0);
  let diff = Math.round((now - then) / 1000);
  if (diff < 0) diff += 86400; // stamp was yesterday
  if (diff < 10) return 'just now';
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return stamp;
}

function renderTranscripts(transcripts) {
  const list = document.getElementById('transcript-list');
  if (transcripts.length === 0) {
    list.innerHTML = '<p class="empty-state">Nothing spoken yet.<br>Hold your hotkey and start talking.</p>';
    return;
  }
  list.innerHTML = transcripts.map((t, i) => {
    const mode = t.mode_used || 'dictate';
    return `
    <div class="transcript-entry" data-mode="${escapeHtml(mode)}" data-index="${i}">
      <div class="meta">
        <span class="mode-tag">${escapeHtml(mode)}</span>
        <span class="time">${relativeTime(t.timestamp)}</span>
        <div class="entry-actions">
          <button class="icon-btn" data-act="copy" title="Copy">⧉</button>
          <button class="icon-btn" data-act="delete" title="Delete">✕</button>
        </div>
      </div>
      <div class="output">${escapeHtml(t.cleaned_text)}</div>
    </div>`;
  }).join('');

  // Wire per-card actions.
  list.querySelectorAll('.transcript-entry').forEach(card => {
    const idx = Number(card.dataset.index);
    card.querySelector('[data-act=copy]').addEventListener('click', async () => {
      const text = transcripts[idx].cleaned_text;
      try {
        await navigator.clipboard.writeText(text);
      } catch { /* clipboard may be unavailable; ignore */ }
      flashCard(card, 'Copied');
    });
    card.querySelector('[data-act=delete]').addEventListener('click', async () => {
      try {
        await invoke('delete_transcript', { index: idx });
        await loadTranscripts();
      } catch (e) { console.error('delete failed:', e); }
    });
  });
}

// Briefly show a label on a card (e.g. "Copied").
function flashCard(card, label) {
  const tag = card.querySelector('.mode-tag');
  if (!tag) return;
  const prev = tag.textContent;
  tag.textContent = label;
  setTimeout(() => { tag.textContent = prev; }, 900);
}

async function loadDictionary() {
  try {
    const entries = await invoke('get_dictionary');
    renderDictionary(entries);
  } catch (e) {
    console.error('failed to load dictionary:', e);
  }
}

function renderDictionary(entries) {
  const list = document.getElementById('dictionary-list');
  const items = Object.entries(entries);
  if (items.length === 0) {
    list.innerHTML = '<p class="empty-state" style="padding:8px 0">No dictionary entries.</p>';
    return;
  }
  list.innerHTML = items.map(([term, expansion]) => `
    <div class="dict-row">
      <span class="term">${escapeHtml(term)}</span>
      <span class="expansion">→ ${escapeHtml(expansion)}</span>
      <span class="remove" data-term="${escapeHtml(term)}">×</span>
    </div>
  `).join('');

  list.querySelectorAll('.remove').forEach(btn => {
    btn.addEventListener('click', async () => {
      await invoke('remove_dictionary_entry', { term: btn.dataset.term });
      await loadDictionary();
    });
  });
}

async function loadModes() {
  try {
    const [modes, builtins] = await Promise.all([
      invoke('get_modes'),
      invoke('get_builtin_mode_names'),
    ]);
    const builtinSet = new Set(builtins);
    const list = document.getElementById('modes-list');
    list.innerHTML = modes.map(m => {
      const custom = !builtinSet.has(m.name);
      return `
      <div class="mode-item">
        <span class="mode-name">${escapeHtml(m.name)}</span>
        <span class="mode-triggers">${m.triggers.map(t => `"${escapeHtml(t)}"`).join(', ')}</span>
        ${custom ? `<span class="remove" data-mode="${escapeHtml(m.name)}" title="Remove">×</span>` : ''}
      </div>`;
    }).join('');

    list.querySelectorAll('.remove').forEach(btn => {
      btn.addEventListener('click', async () => {
        await invoke('remove_mode', { name: btn.dataset.mode });
        await loadModes();
      });
    });
  } catch (e) {
    console.error('failed to load modes:', e);
  }
}

// Merge the form's fields into the loaded config and persist. We never rebuild
// the whole config from scratch, so settings the form doesn't expose (modes,
// stt.model_path, dictionary learning, etc.) are preserved.
async function saveConfig() {
  const base = currentConfig || {};
  const config = {
    ...base,
    llm: {
      ...(base.llm || {}),
      protocol: document.getElementById('llm-protocol').value || null,
      endpoint: document.getElementById('llm-endpoint').value,
      api_key: document.getElementById('llm-apikey').value || null,
      region: document.getElementById('llm-region').value || null,
      cleanup_model: document.getElementById('llm-cleanup-model').value,
      command_model: document.getElementById('llm-command-model').value,
    },
    hotkeys: {
      ...(base.hotkeys || {}),
      dictate: document.getElementById('hotkey-dictate').value,
      command: document.getElementById('hotkey-command').value,
    },
    paste: {
      ...(base.paste || {}),
      preview_before_paste: document.getElementById('preview-before-paste').checked,
    },
    dictionary: {
      ...(base.dictionary || {}),
      codebase_path: document.getElementById('codebase-path').value.trim() || null,
    },
  };
  await invoke('save_config', { config });
  currentConfig = config;
}

function setupListeners() {
  // Reveal only the fields the chosen provider uses
  document.getElementById('llm-protocol').addEventListener('change', applyProtocolFields);

  // Save settings
  document.getElementById('save-btn').addEventListener('click', async () => {
    try {
      await saveConfig();
      document.getElementById('save-btn').textContent = 'Saved!';
      setTimeout(() => { document.getElementById('save-btn').textContent = 'Save Settings'; }, 1500);
    } catch (e) {
      alert('Save failed: ' + e);
    }
  });

  // Clear transcripts
  document.getElementById('clear-btn').addEventListener('click', async () => {
    await invoke('clear_transcripts');
    await loadTranscripts();
  });

  // Add dictionary entry
  document.getElementById('dict-add-btn').addEventListener('click', async () => {
    const term = document.getElementById('dict-term').value.trim();
    const expansion = document.getElementById('dict-expansion').value.trim();
    if (term && expansion) {
      await invoke('add_dictionary_entry', { term, expansion });
      document.getElementById('dict-term').value = '';
      document.getElementById('dict-expansion').value = '';
      await loadDictionary();
    }
  });

  // Add custom mode
  document.getElementById('mode-add-btn').addEventListener('click', async () => {
    const name = document.getElementById('mode-name').value.trim();
    const triggers = document.getElementById('mode-triggers').value
      .split(',').map(t => t.trim()).filter(Boolean);
    const template = document.getElementById('mode-template').value;
    if (!name || triggers.length === 0 || !template.trim()) {
      alert('A mode needs a name, at least one trigger, and a template.');
      return;
    }
    try {
      await invoke('add_mode', { name, triggers, template });
      document.getElementById('mode-name').value = '';
      document.getElementById('mode-triggers').value = '';
      document.getElementById('mode-template').value = '';
      await loadModes();
    } catch (e) {
      alert('Add mode failed: ' + e);
    }
  });

  // Onboarding banner
  document.getElementById('onboarding-dismiss').addEventListener('click', () => {
    localStorage.setItem('onboarding-dismissed', '1');
    document.getElementById('onboarding').hidden = true;
  });
  document.getElementById('open-privacy').addEventListener('click', () => {
    invoke('open_privacy_settings');
  });
  document.getElementById('goto-check').addEventListener('click', async () => {
    document.querySelector('[data-tab=settings]').click();
    document.getElementById('check-btn').click();
  });

  // Re-index codebase (saves the path first so the backend reads the latest)
  document.getElementById('reindex-btn').addEventListener('click', async () => {
    const status = document.getElementById('reindex-status');
    status.textContent = 'Indexing…';
    try {
      await saveConfig();               // persist the path first
      const result = await invoke('reindex_codebase');
      status.textContent = result;
    } catch (e) {
      status.textContent = String(e);
    }
  });

  // System check
  document.getElementById('check-btn').addEventListener('click', async () => {
    try {
      const results = await invoke('check_system');
      document.getElementById('check-output').textContent = results.join('\n');
    } catch (e) {
      document.getElementById('check-output').textContent = 'Error: ' + e;
    }
  });

  // Tauri events
  listen('recording-started', () => {
    document.getElementById('status').className = 'status recording';
  });
  listen('recording-stopped', () => {
    document.getElementById('status').className = 'status processing';
  });
  listen('transcript-added', async (event) => {
    document.getElementById('status').className = 'status idle';
    await loadTranscripts();
  });
  listen('processing-error', (event) => {
    // The pill shows the error at the cursor; here we just blip the status dot
    // and return to idle. No separate toast — keep the surface minimal.
    const status = document.getElementById('status');
    status.className = 'status error';
    console.error('processing error:', event.payload);
    setTimeout(() => { status.className = 'status idle'; }, 2500);
  });
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

document.addEventListener('DOMContentLoaded', init);
