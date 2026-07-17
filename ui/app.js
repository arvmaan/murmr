const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// Tab switching
document.querySelectorAll('.tab').forEach(tab => {
  tab.addEventListener('click', () => {
    document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
    tab.classList.add('active');
    document.getElementById(tab.dataset.tab).classList.add('active');
  });
});

// Load initial state
async function init() {
  await loadConfig();
  await loadTranscripts();
  await loadDictionary();
  await loadModes();
  setupListeners();
}

async function loadConfig() {
  try {
    const config = await invoke('get_config');
    document.getElementById('llm-protocol').value = config.llm.protocol || 'ollama';
    document.getElementById('llm-endpoint').value = config.llm.endpoint || '';
    document.getElementById('llm-apikey').value = config.llm.api_key || '';
    document.getElementById('llm-region').value = config.llm.region || '';
    document.getElementById('llm-cleanup-model').value = config.llm.cleanup_model || '';
    document.getElementById('llm-command-model').value = config.llm.command_model || '';
    document.getElementById('hotkey-dictate').value = config.hotkeys.dictate || '';
    document.getElementById('hotkey-command').value = config.hotkeys.command || '';
  } catch (e) {
    console.error('failed to load config:', e);
  }
}

async function loadTranscripts() {
  try {
    const transcripts = await invoke('get_transcripts');
    renderTranscripts(transcripts);
  } catch (e) {
    console.error('failed to load transcripts:', e);
  }
}

function renderTranscripts(transcripts) {
  const list = document.getElementById('transcript-list');
  if (transcripts.length === 0) {
    list.innerHTML = '<p class="empty-state">No transcriptions yet. Press your hotkey to dictate.</p>';
    return;
  }
  list.innerHTML = transcripts.map(t => `
    <div class="transcript-entry">
      <div class="meta">
        <span>${t.timestamp}</span>
        ${t.mode_used ? `<span class="mode-badge">${t.mode_used}</span>` : ''}
      </div>
      <div class="output">${escapeHtml(t.cleaned_text)}</div>
    </div>
  `).join('');
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
    const modes = await invoke('get_modes');
    const list = document.getElementById('modes-list');
    list.innerHTML = modes.map(m => `
      <div class="mode-item">
        <span class="mode-name">${escapeHtml(m.name)}</span>
        <span class="mode-triggers">${m.triggers.map(t => `"${t}"`).join(', ')}</span>
      </div>
    `).join('');
  } catch (e) {
    console.error('failed to load modes:', e);
  }
}

function setupListeners() {
  // Save settings
  document.getElementById('save-btn').addEventListener('click', async () => {
    const config = {
      llm: {
        protocol: document.getElementById('llm-protocol').value || null,
        endpoint: document.getElementById('llm-endpoint').value,
        api_key: document.getElementById('llm-apikey').value || null,
        region: document.getElementById('llm-region').value || null,
        cleanup_model: document.getElementById('llm-cleanup-model').value,
        command_model: document.getElementById('llm-command-model').value,
        cleanup_prompt: null,
      },
      hotkeys: {
        dictate: document.getElementById('hotkey-dictate').value,
        command: document.getElementById('hotkey-command').value,
      },
      stt: { model_path: '', language: 'en' },
      paste: { method: 'auto' },
      modes: [],
      dictionary: { entries: {}, learning: { enabled: true, suggestion_threshold: 3 } },
    };
    try {
      await invoke('save_config', { config });
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
    document.getElementById('status').className = 'status idle';
    console.error('processing error:', event.payload);
  });
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

document.addEventListener('DOMContentLoaded', init);
