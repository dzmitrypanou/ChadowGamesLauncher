import { invoke } from '@tauri-apps/api/core';
import appVersionData from '../config/version.json';

const APP_VERSION = appVersionData.version || '3.2.2 Creeper';

const DEFAULT_API = 'https://chadow.ru/api/minecraft/bootstrap';

const els = {
  nickname: document.getElementById('nickname'),
  nicknameWarn: document.getElementById('nicknameWarn'),
  wgBtn: document.getElementById('wgBtn'),
  lestaBtn: document.getElementById('lestaBtn'),
  gamesTableBody: document.getElementById('gamesTableBody'),
  progressWrap: document.getElementById('progressWrap'),
  progressFill: document.getElementById('progressFill'),
  progressText: document.getElementById('progressText'),
  playBtn: document.getElementById('playBtn'),
  settingsBtn: document.getElementById('settingsBtn'),
  settingsModal: document.getElementById('settingsModal'),
  settingsBackdrop: document.getElementById('settingsBackdrop'),
  settingsCloseBtn: document.getElementById('settingsCloseBtn'),
  settingsSaveBtn: document.getElementById('settingsSaveBtn'),
  ramSlider: document.getElementById('ramSlider'),
  ramLabel: document.getElementById('ramLabel'),
  launcherVersion: document.getElementById('launcherVersion'),
  minimizeBtn: document.getElementById('minimizeBtn'),
  closeBtn: document.getElementById('closeBtn'),
};

/** @type {Record<string, unknown>|null} */
let bootstrap = null;
/** @type {{ nickname: string, ramGb: number, apiUrl: string }|null} */
let profile = null;
let busy = false;
/** @type {ReturnType<typeof setInterval>|null} */
let pingTimer = null;
/** @type {ReturnType<typeof setInterval>|null} */
let oauthPollTimer = null;
let oauthBusy = false;
let gameRunning = false;

const PLAY_LABEL_IDLE = '▶ Играть';
const PLAY_LABEL_RUNNING = '● Запущено';

/** @type {Array<{ key: string, gameName: string, serverName: string, host: string, port: number }>} */
let gameRows = [];

function setProgress(visible, percent = 0, text = '') {
  els.progressWrap.hidden = !visible;
  els.progressFill.style.width = `${Math.max(0, Math.min(100, percent))}%`;
  els.progressText.textContent = text;
}

const MINECRAFT_NICK_MAX = 16;

function nicknameSanitized(value) {
  return String(value).trim().replace(/[^a-zA-Z0-9_]/g, '');
}

function validNickname(value) {
  return /^[a-zA-Z0-9_]{3,24}$/.test(value);
}

function minecraftLaunchUsername(value) {
  const nick = nicknameSanitized(value);
  if (nick.length < 3) return '';
  return nick.slice(0, MINECRAFT_NICK_MAX);
}

function updateNicknameWarning() {
  if (!els.nicknameWarn) return;

  const field = els.nickname?.closest('.nickname-field');
  const nick = els.nickname.value.trim();
  const exceeds = nick.length > MINECRAFT_NICK_MAX;

  if (field) {
    field.classList.toggle('nickname-field--warn', exceeds);
  }

  if (exceeds) {
    const truncated = minecraftLaunchUsername(nick);
    const tip = `Minecraft допускает не более ${MINECRAFT_NICK_MAX} символов в нике. При запуске будет использован: «${truncated}». Рекомендуем сменить ник до входа в игру.`;
    els.nicknameWarn.hidden = false;
    els.nicknameWarn.setAttribute('aria-label', tip);
    const tipEl = els.nicknameWarn.querySelector('.nickname-warn-tip');
    if (tipEl) tipEl.textContent = tip;
    return;
  }

  els.nicknameWarn.hidden = true;
  els.nicknameWarn.removeAttribute('aria-label');
}

function setPlayButtonRunning(running) {
  gameRunning = running;
  if (els.playBtn) {
    els.playBtn.textContent = running ? PLAY_LABEL_RUNNING : PLAY_LABEL_IDLE;
    els.playBtn.classList.toggle('btn-play--running', running);
  }
  updatePlayState();
}

function updatePlayState() {
  updateNicknameWarning();
  if (gameRunning) {
    els.playBtn.disabled = true;
    return;
  }
  const nick = els.nickname.value.trim();
  const ready = !busy && !oauthBusy && bootstrap?.enabled && validNickname(nick);
  els.playBtn.disabled = !ready;
}

function getApiUrl() {
  return DEFAULT_API;
}

function getOAuthApiBase() {
  return getApiUrl().replace(/\/bootstrap(\.php)?(\?.*)?$/i, '');
}

async function loadProfile() {
  profile = await invoke('load_profile');
  if (profile?.nickname) els.nickname.value = profile.nickname;
  if (profile?.ramGb) {
    els.ramSlider.value = String(profile.ramGb);
    els.ramLabel.textContent = String(profile.ramGb);
  }
}

async function saveProfile() {
  profile = {
    nickname: els.nickname.value.trim(),
    ramGb: Number(els.ramSlider.value),
    apiUrl: DEFAULT_API,
  };
  await invoke('save_profile', { profile });
}

async function fetchBootstrap() {
  return invoke('fetch_bootstrap', { apiUrl: getApiUrl() });
}

function formatSlots(online, max) {
  if (max <= 0) return '—';
  return `${Math.max(0, online)} / ${max}`;
}

function renderGamesTable(message = null) {
  if (!els.gamesTableBody) return;

  if (message) {
    els.gamesTableBody.innerHTML = `
      <tr class="games-row games-row-empty">
        <td colspan="4">${escapeHtml(message)}</td>
      </tr>`;
    gameRows = [];
    return;
  }

  if (!gameRows.length) {
    renderGamesTable('Серверы не настроены');
    return;
  }

  els.gamesTableBody.innerHTML = gameRows.map(row => `
    <tr class="games-row games-row-pending" data-row-key="${escapeHtml(row.key)}">
      <td>${escapeHtml(row.gameName)}</td>
      <td>${escapeHtml(row.serverName)}</td>
      <td class="games-slots" data-field="slots">—</td>
      <td class="games-ping" data-field="ping">проверка…</td>
    </tr>
  `).join('');
}

function setRowPingLoading(key) {
  const row = els.gamesTableBody?.querySelector(`tr[data-row-key="${key}"]`);
  if (!row) return;

  row.classList.remove('games-row-online', 'games-row-offline');
  row.classList.add('games-row-pending');

  const slotsCell = row.querySelector('[data-field="slots"]');
  const pingCell = row.querySelector('[data-field="ping"]');
  if (slotsCell) slotsCell.textContent = '—';
  if (pingCell) pingCell.textContent = 'проверка…';
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function updateRowPing(key, result) {
  const row = els.gamesTableBody?.querySelector(`tr[data-row-key="${key}"]`);
  if (!row) return;

  const slotsCell = row.querySelector('[data-field="slots"]');
  const pingCell = row.querySelector('[data-field="ping"]');
  if (!slotsCell || !pingCell) return;

  row.classList.remove('games-row-pending');

  if (result.online) {
    row.classList.remove('games-row-offline');
    row.classList.add('games-row-online');
    slotsCell.textContent = formatSlots(result.playersOnline, result.playersMax);
    pingCell.textContent = `${result.latencyMs} ms`;
  } else {
    row.classList.remove('games-row-online');
    row.classList.add('games-row-offline');
    slotsCell.textContent = '—';
    pingCell.textContent = 'offline';
  }
}

async function pingAllServers() {
  for (const row of gameRows) {
    setRowPingLoading(row.key);
  }

  await Promise.all(gameRows.map(async row => {
    try {
      const result = await invoke('ping_server', { host: row.host, port: row.port });
      updateRowPing(row.key, result);
    } catch {
      updateRowPing(row.key, { online: false, playersOnline: 0, playersMax: 0, latencyMs: 0 });
    }
  }));
}

function schedulePing() {
  if (pingTimer) clearInterval(pingTimer);
  if (!gameRows.length) return;
  void pingAllServers();
  pingTimer = setInterval(() => void pingAllServers(), 20000);
}

function collectGameRows(data) {
  const rows = [];
  const games = Array.isArray(data.games) && data.games.length
    ? data.games
    : [{
        id: 'minecraft',
        name: 'Minecraft',
        servers: Array.isArray(data.servers) ? data.servers : [],
      }];

  for (const game of games) {
    const gameName = String(game.name || game.id || 'Игра');
    const servers = Array.isArray(game.servers) ? game.servers : [];
    for (const server of servers) {
      if (!server?.host) continue;
      const serverId = String(server.id || server.host);
      rows.push({
        key: `${game.id || gameName}:${serverId}`,
        gameName,
        serverName: String(server.name || server.host),
        host: String(server.host),
        port: Number(server.port) || 25565,
      });
    }
  }

  return rows;
}

function applyLauncherVersion(version) {
  if (!els.launcherVersion) return;
  els.launcherVersion.textContent = String(version || APP_VERSION).trim();
}

function applyBootstrap(data) {
  bootstrap = data;
  applyLauncherVersion(data.appVersion || APP_VERSION);

  els.wgBtn.disabled = oauthBusy || !(data.oauth?.wg?.enabled);
  els.lestaBtn.disabled = oauthBusy || !(data.oauth?.lesta?.enabled);

  if (!data.enabled) {
    renderGamesTable('Лаунчер отключён администратором');
    return;
  }

  gameRows = collectGameRows(data);
  renderGamesTable();
  schedulePing();
}

async function refreshBootstrap() {
  try {
    const cached = await invoke('load_cached_bootstrap');
    if (cached) {
      applyBootstrap(cached);
    }
  } catch {
    // ignore cache read errors
  }

  try {
    const data = await fetchBootstrap();
    applyBootstrap(data);
    await invoke('cache_bootstrap', { payload: data });
  } catch {
    if (!bootstrap) {
      renderGamesTable('Нет связи с API');
      bootstrap = null;
    }
  } finally {
    updatePlayState();
  }
}

async function openExternalUrl(url) {
  try {
    await invoke('open_url', { url });
  } catch {
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open(url);
    } catch {
      window.open(url, '_blank');
    }
  }
}

function setOAuthBusy(active, provider = '') {
  oauthBusy = active;
  if (bootstrap) {
    els.wgBtn.disabled = active || !(bootstrap.oauth?.wg?.enabled);
    els.lestaBtn.disabled = active || !(bootstrap.oauth?.lesta?.enabled);
  }
  if (active && provider) {
    const btn = provider === 'lesta' ? els.lestaBtn : els.wgBtn;
    if (btn) btn.textContent = '…';
  } else {
    if (els.wgBtn) els.wgBtn.textContent = 'Wargaming API';
    if (els.lestaBtn) els.lestaBtn.textContent = 'Lesta API';
  }
  updatePlayState();
}

async function startOAuth(provider) {
  if (oauthBusy || busy) return;

  const startUrl = bootstrap?.oauth?.[provider]?.startUrl
    || `${getOAuthApiBase()}/oauth/start?provider=${encodeURIComponent(provider)}`;

  setOAuthBusy(true, provider);
  if (els.wgBtn) els.wgBtn.title = '';
  if (els.lestaBtn) els.lestaBtn.title = '';

  if (oauthPollTimer) {
    clearInterval(oauthPollTimer);
    oauthPollTimer = null;
  }

  try {
    const res = await fetch(startUrl);
    const raw = await res.text();
    let data = null;
    try {
      data = raw ? JSON.parse(raw) : null;
    } catch {
      data = null;
    }

    if (!res.ok || !data?.success || !data.loginUrl || !data.session) {
      const message = data?.error || (res.ok ? 'Не удалось начать вход' : `Ошибка API (${res.status})`);
      const btn = provider === 'lesta' ? els.lestaBtn : els.wgBtn;
      if (btn) btn.title = message;
      setProgress(true, 0, message);
      setOAuthBusy(false);
      setTimeout(() => setProgress(false), 5000);
      return;
    }

    await openExternalUrl(data.loginUrl);
    setProgress(true, 0, 'Завершите вход в браузере…');

    const session = data.session;
    const pollBase = `${getOAuthApiBase()}/oauth/poll`;
    const startedAt = Date.now();

    oauthPollTimer = setInterval(async () => {
      if (Date.now() - startedAt > 600000) {
        clearInterval(oauthPollTimer);
        oauthPollTimer = null;
        setOAuthBusy(false);
        return;
      }

      try {
        const pollRes = await fetch(`${pollBase}?session=${encodeURIComponent(session)}`);
        const poll = await pollRes.json();

        if (poll.status === 'done' && poll.nickname) {
          clearInterval(oauthPollTimer);
          oauthPollTimer = null;
          els.nickname.value = poll.nickname;
          await saveProfile();
          setProgress(false);
          setOAuthBusy(false);
          updatePlayState();
          return;
        }

        if (poll.status === 'error' || poll.status === 'expired') {
          clearInterval(oauthPollTimer);
          oauthPollTimer = null;
          const btn = provider === 'lesta' ? els.lestaBtn : els.wgBtn;
          const message = poll.error || 'Ошибка авторизации';
          if (btn) btn.title = message;
          setProgress(true, 0, message);
          setOAuthBusy(false);
          setTimeout(() => setProgress(false), 5000);
        }
      } catch {
        // keep polling
      }
    }, 2000);
  } catch {
    setOAuthBusy(false);
  }
}

async function handlePlay() {
  if (busy || !bootstrap?.enabled) return;
  const nickname = els.nickname.value.trim();
  const launchNick = minecraftLaunchUsername(nickname);
  if (!validNickname(nickname) || launchNick.length < 3) return;

  busy = true;
  updatePlayState();
  setProgress(true, 0, 'Подготовка…');

  try {
    await saveProfile();
    const result = await invoke('prepare_and_launch', {
      nickname: launchNick,
      ramGb: Number(els.ramSlider.value),
      bootstrap,
    });

    if (result?.launched) {
      setProgress(false);
      setPlayButtonRunning(true);
    }
  } catch (err) {
    const message = String(err || 'Не удалось запустить игру');
    setProgress(true, 0, message);
    setTimeout(() => setProgress(false), 12000);
  } finally {
    busy = false;
    updatePlayState();
  }
}

els.nickname.addEventListener('input', updatePlayState);
els.nickname.addEventListener('keydown', e => {
  if (e.key === 'Enter') handlePlay();
});
els.ramSlider.addEventListener('input', () => {
  els.ramLabel.textContent = els.ramSlider.value;
});
els.playBtn.addEventListener('click', handlePlay);
function openSettings() {
  if (!els.settingsModal) return;
  els.settingsModal.hidden = false;
  els.settingsModal.setAttribute('aria-hidden', 'false');
}

async function closeSettings(save = true) {
  if (!els.settingsModal) return;
  if (save) {
    try {
      await saveProfile();
    } catch {
      // ignore save errors
    }
  }
  els.settingsModal.hidden = true;
  els.settingsModal.setAttribute('aria-hidden', 'true');
}

els.settingsBtn.addEventListener('click', () => openSettings());
els.settingsBackdrop?.addEventListener('click', () => closeSettings());
els.settingsCloseBtn?.addEventListener('click', () => closeSettings());
els.settingsSaveBtn?.addEventListener('click', () => closeSettings());
document.addEventListener('keydown', e => {
  if (e.key === 'Escape' && els.settingsModal && !els.settingsModal.hidden) {
    closeSettings();
  }
});
els.wgBtn.addEventListener('click', () => startOAuth('wg'));
els.lestaBtn.addEventListener('click', () => startOAuth('lesta'));

async function setupWindowControls() {
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const appWindow = getCurrentWindow();
    els.minimizeBtn?.addEventListener('click', () => appWindow.minimize());
    els.closeBtn?.addEventListener('click', () => appWindow.close());
  } catch {
    els.minimizeBtn?.style.setProperty('display', 'none');
    els.closeBtn?.style.setProperty('display', 'none');
  }
}

async function init() {
  await setupWindowControls();

  try {
    const { listen } = await import('@tauri-apps/api/event');
    await listen('install-progress', ({ payload }) => {
      const p = /** @type {{ percent: number, message: string }} */ (payload);
      setProgress(true, p.percent, p.message);
    });
    await listen('game-exited', () => {
      setPlayButtonRunning(false);
    });
  } catch {
    // Browser preview
  }

  try {
    const running = await invoke('game_is_running');
    if (running) setPlayButtonRunning(true);
  } catch {
    // ignore
  }

  try {
    await loadProfile();
  } catch {
    // ignore profile load errors
  }
  applyLauncherVersion(APP_VERSION);
  await refreshBootstrap();
}

init().catch(() => {
  renderGamesTable('Ошибка запуска');
});
