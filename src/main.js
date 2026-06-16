import { invoke } from '@tauri-apps/api/core';
import appVersionData from '../config/version.json';

document.addEventListener('contextmenu', (event) => {
  event.preventDefault();
}, { capture: true });

const APP_VERSION = appVersionData.version || '3.2.2 Creeper';
const DEFAULT_API = 'https://chadow.ru/api/minecraft/bootstrap';
const MINECRAFT_NICK_MAX = 16;

const PLAY_LABEL_IDLE = 'Играть';
const PLAY_LABEL_UPDATE = 'Обновить';
const PLAY_LABEL_RUNNING = 'Запущено';
const DISPLAY_MODE_WINDOWED = 'windowed';
const DISPLAY_MODE_FULLSCREEN = 'fullscreen';

/** Games shown before API support exists */
const PLACEHOLDER_GAMES = [
  {
    id: 'samp',
    name: 'Неизвестно',
    subtitle: 'Данные отсутствуют',
    playable: false,
    accent: '#6b7280',
  },
];

const GAME_VISUALS = {
  minecraft: {
    subtitle: 'Java Edition',
    accent: '#74f6c8',
    glyph: '⛏',
  },
  samp: {
    subtitle: 'Данные отсутствуют',
    accent: '#6b7280',
    glyph: '?',
  },
};

const els = {
  nickname: document.getElementById('nickname'),
  nicknameWarn: document.getElementById('nicknameWarn'),
  gameGrid: document.getElementById('gameGrid'),
  serverPanel: document.getElementById('serverPanel'),
  serverPanelTitle: document.getElementById('serverPanelTitle'),
  serverList: document.getElementById('serverList'),
  statusHint: document.getElementById('statusHint'),
  progressWrap: document.getElementById('progressWrap'),
  progressFill: document.getElementById('progressFill'),
  playBtn: document.getElementById('playBtn'),
  playBtnLabel: document.getElementById('playBtnLabel'),
  settingsBtn: document.getElementById('settingsBtn'),
  settingsModal: document.getElementById('settingsModal'),
  settingsBackdrop: document.getElementById('settingsBackdrop'),
  settingsCloseBtn: document.getElementById('settingsCloseBtn'),
  settingsSaveBtn: document.getElementById('settingsSaveBtn'),
  clearDataBtn: document.getElementById('clearDataBtn'),
  installPathsList: document.getElementById('installPathsList'),
  launcherVersion: document.getElementById('launcherVersion'),
  minimizeBtn: document.getElementById('minimizeBtn'),
  closeBtn: document.getElementById('closeBtn'),
};

/** @type {Record<string, unknown>|null} */
let bootstrap = null;
/** @type {{ nickname: string, apiUrl: string, gameInstallPaths?: Record<string, string>, selectedServers?: Record<string, string> }|null} */
let profile = null;
let busy = false;
/** @type {ReturnType<typeof setInterval>|null} */
let pingTimer = null;
let gameRunning = false;

/** @type {Array<{ id: string, name: string, subtitle: string, playable: boolean, badge?: string, accent: string, glyph: string, servers: Array<{ key: string, name: string, host: string, port: number }> }>} */
let gameCatalog = [];
let selectedGameId = 'minecraft';
/** @type {Record<string, string>} */
let selectedServerKeys = {};
let clientPackNeedsUpdate = false;

function computePlayButtonLabel() {
  if (gameRunning) return PLAY_LABEL_RUNNING;
  return clientPackNeedsUpdate ? PLAY_LABEL_UPDATE : PLAY_LABEL_IDLE;
}

function getDisplayMode() {
  const selected = document.querySelector('input[name="displayMode"]:checked');
  return selected?.value === DISPLAY_MODE_FULLSCREEN
    ? DISPLAY_MODE_FULLSCREEN
    : DISPLAY_MODE_WINDOWED;
}

function applyDisplayModeToUi(mode) {
  const value = mode === DISPLAY_MODE_FULLSCREEN
    ? DISPLAY_MODE_FULLSCREEN
    : DISPLAY_MODE_WINDOWED;
  const input = document.querySelector(`input[name="displayMode"][value="${value}"]`);
  if (input) input.checked = true;
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function setProgress(visible, percent = 0, text = '') {
  const pct = Math.max(0, Math.min(100, Math.round(percent)));
  const message = text || 'Подготовка…';

  if (els.progressWrap) {
    els.progressWrap.hidden = !visible;
  }
  if (els.progressFill) els.progressFill.style.width = `${pct}%`;
  if (els.statusHint) {
    els.statusHint.classList.toggle('status-hint--loading', visible);
    els.statusHint.textContent = visible ? `${message} · ${pct}%` : '';
    if (!visible) updateStatusHint();
  }

  const bar = els.progressWrap?.querySelector('.progress-strip-bar');
  if (bar) {
    bar.setAttribute('aria-valuenow', String(pct));
    bar.setAttribute('aria-valuetext', `${pct}% — ${message}`);
  }
}

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
    const tip = `Minecraft допускает не более ${MINECRAFT_NICK_MAX} символов. При запуске будет: «${truncated}».`;
    els.nicknameWarn.hidden = false;
    els.nicknameWarn.setAttribute('aria-label', tip);
    const tipEl = els.nicknameWarn.querySelector('.nickname-warn-tip');
    if (tipEl) tipEl.textContent = tip;
    return;
  }

  els.nicknameWarn.hidden = true;
  els.nicknameWarn.removeAttribute('aria-label');
}

function selectedGame() {
  return gameCatalog.find(g => g.id === selectedGameId) || null;
}

function ensureServerSelection(game) {
  if (!game?.servers.length) return;

  const saved = selectedServerKeys[game.id] || profile?.selectedServers?.[game.id];
  const match = game.servers.find(server => server.id === saved || server.key === saved);
  selectedServerKeys[game.id] = match?.id || game.servers[0].id;
}

function getSelectedServer(game = selectedGame()) {
  if (!game?.servers.length) return null;
  ensureServerSelection(game);
  const selectedId = selectedServerKeys[game.id];
  return game.servers.find(server => server.id === selectedId) || game.servers[0];
}

function selectServer(gameId, serverId) {
  if (selectedServerKeys[gameId] === serverId) return;
  selectedServerKeys[gameId] = serverId;
  updateServerSelectionUi(gameId);
  updatePlayState();
}

function updateServerSelectionUi(gameId = selectedGameId) {
  const selectedId = selectedServerKeys[gameId];
  els.serverList?.querySelectorAll('.server-card').forEach(card => {
    const isSelected = card.getAttribute('data-server-id') === selectedId;
    card.classList.toggle('server-card--selected', isSelected);
    card.setAttribute('aria-pressed', isSelected ? 'true' : 'false');
  });
}

function canLaunchSelectedGame() {
  const game = selectedGame();
  return Boolean(
    game?.playable
    && game.id === 'minecraft'
    && bootstrap?.enabled
    && getSelectedServer(game),
  );
}

function setPlayButtonRunning(running) {
  gameRunning = running;
  if (els.playBtnLabel) {
    els.playBtnLabel.textContent = computePlayButtonLabel();
  }
  if (els.playBtn) {
    els.playBtn.classList.toggle('btn-play--running', running);
  }
  updatePlayState();
}

function updateStatusHint() {
  if (!els.statusHint) return;

  if (gameRunning) {
    els.statusHint.textContent = 'Игра запущена — закройте клиент, чтобы сыграть снова';
    return;
  }

  if (busy) {
    els.statusHint.textContent = 'Подготовка клиента…';
    return;
  }

  const game = selectedGame();
  if (!game) {
    els.statusHint.textContent = 'Загрузка списка игр…';
    return;
  }

  if (!game.playable) {
    els.statusHint.textContent = game.subtitle || `${game.name} недоступна`;
    return;
  }

  if (!bootstrap?.enabled) {
    els.statusHint.textContent = 'Лаунчер временно отключён';
    return;
  }

  const nick = els.nickname.value.trim();
  if (!validNickname(nick)) {
    els.statusHint.textContent = 'Введите никнейм (3–24 символа: латиница, цифры, _)';
    return;
  }

  const server = getSelectedServer(game);
  els.statusHint.textContent = `Готово к запуску — ${game.name}${server ? ` · ${server.name}` : ''}`;
}

function updatePlayState() {
  updateNicknameWarning();
  updateStatusHint();

  if (els.playBtnLabel) {
    els.playBtnLabel.textContent = computePlayButtonLabel();
  }

  if (gameRunning) {
    els.playBtn.disabled = true;
    return;
  }

  const nick = els.nickname.value.trim();
  const ready = !busy && canLaunchSelectedGame() && validNickname(nick);
  els.playBtn.disabled = !ready;
}

function getApiUrl() {
  return DEFAULT_API;
}

async function loadProfile() {
  profile = await invoke('load_profile');
  if (profile?.nickname) els.nickname.value = profile.nickname;
  if (profile?.selectedServers) {
    selectedServerKeys = { ...profile.selectedServers };
  }
  applyDisplayModeToUi(profile?.displayMode || DISPLAY_MODE_WINDOWED);
}

async function saveProfile() {
  profile = {
    nickname: els.nickname.value.trim(),
    apiUrl: DEFAULT_API,
    gameInstallPaths: profile?.gameInstallPaths || {},
    selectedServers: { ...selectedServerKeys },
    displayMode: getDisplayMode(),
  };
  await invoke('save_profile', { profile });
}

async function fetchBootstrap() {
  return invoke('fetch_bootstrap', { apiUrl: getApiUrl() });
}

function formatSlots(online, max) {
  if (max <= 0) return '—';
  return `${Math.max(0, online)}/${max}`;
}

function gameVisual(id, fallbackName) {
  const preset = GAME_VISUALS[id] || {};
  return {
    subtitle: preset.subtitle || 'Chadow Games',
    accent: preset.accent || '#64b5f6',
    glyph: preset.glyph || fallbackName?.charAt(0)?.toUpperCase() || '?',
  };
}

function buildGameCatalog(data) {
  const enabled = Boolean(data?.enabled);
  const catalog = [];

  const games = Array.isArray(data?.games) && data.games.length
    ? data.games
    : [{
        id: 'minecraft',
        name: 'Minecraft',
        servers: Array.isArray(data?.servers) ? data.servers : [],
      }];

  for (const game of games) {
    const id = String(game.id || 'minecraft');
    const name = String(game.name || id);
    const visual = gameVisual(id, name);
    const servers = [];

    for (const server of Array.isArray(game.servers) ? game.servers : []) {
      if (!server?.host) continue;
      const serverId = String(server.id || server.host);
      servers.push({
        key: `${id}:${serverId}`,
        id: serverId,
        name: String(server.name || server.host),
        host: String(server.host),
        port: Number(server.port) || 25565,
      });
    }

    catalog.push({
      id,
      name,
      subtitle: visual.subtitle,
      playable: id === 'minecraft' && enabled,
      accent: visual.accent,
      glyph: visual.glyph,
      servers,
    });
  }

  for (const placeholder of PLACEHOLDER_GAMES) {
    if (!catalog.some(g => g.id === placeholder.id)) {
      const visual = gameVisual(placeholder.id, placeholder.name);
      catalog.push({
        id: placeholder.id,
        name: placeholder.name,
        subtitle: placeholder.subtitle || visual.subtitle,
        playable: false,
        badge: placeholder.badge,
        accent: placeholder.accent || visual.accent,
        glyph: visual.glyph,
        servers: [],
      });
    }
  }

  return catalog;
}

function renderGameGrid(message = null) {
  if (!els.gameGrid) return;

  if (message) {
    els.gameGrid.innerHTML = `<p class="game-grid-empty">${escapeHtml(message)}</p>`;
    return;
  }

  els.gameGrid.innerHTML = gameCatalog.map(game => {
    const selected = game.id === selectedGameId;
    const muted = !game.playable;
    return `
      <button
        type="button"
        class="game-card${selected ? ' game-card--selected' : ''}${muted ? ' game-card--muted' : ''}"
        data-game-id="${escapeHtml(game.id)}"
        role="option"
        aria-selected="${selected ? 'true' : 'false'}"
        style="--game-accent: ${escapeHtml(game.accent)}"
      >
        <span class="game-card-glow" aria-hidden="true"></span>
        <span class="game-card-icon" aria-hidden="true">${escapeHtml(game.glyph)}</span>
        <span class="game-card-body">
          <span class="game-card-name">${escapeHtml(game.name)}</span>
          <span class="game-card-sub">${escapeHtml(game.subtitle)}</span>
        </span>
        ${game.badge ? `<span class="game-card-badge">${escapeHtml(game.badge)}</span>` : ''}
        ${selected && game.playable ? '<span class="game-card-check" aria-hidden="true">✓</span>' : ''}
      </button>`;
  }).join('');

  els.gameGrid.querySelectorAll('.game-card').forEach(btn => {
    btn.addEventListener('click', async () => {
      const id = btn.getAttribute('data-game-id');
      if (!id || id === selectedGameId) return;
      selectedGameId = id;
      renderGameGrid();
      renderServerPanel();
      schedulePing();
      await refreshClientPackUpdateState();
      updatePlayState();
    });
  });
}

function renderServerPanel() {
  const game = selectedGame();

  if (!game) {
    els.serverPanel.hidden = true;
    els.serverList.innerHTML = '';
    return;
  }

  if (!game.playable) {
    els.serverPanel.hidden = false;
    if (els.serverPanelTitle) {
      els.serverPanelTitle.textContent = game.name;
    }
    els.serverList.innerHTML = `<p class="server-empty">${escapeHtml(game.subtitle || 'Данные отсутствуют')}</p>`;
    return;
  }

  if (!game.servers.length) {
    els.serverPanel.hidden = true;
    els.serverList.innerHTML = '';
    return;
  }

  ensureServerSelection(game);
  const selectedId = selectedServerKeys[game.id];

  els.serverPanel.hidden = false;
  if (els.serverPanelTitle) {
    els.serverPanelTitle.textContent = game.servers.length > 1
      ? `Выберите сервер — ${game.name}`
      : `Сервер — ${game.name}`;
  }

  els.serverList.innerHTML = game.servers.map(server => `
    <button
      type="button"
      class="server-card server-card--pending${server.id === selectedId ? ' server-card--selected' : ''}"
      data-server-key="${escapeHtml(server.key)}"
      data-server-id="${escapeHtml(server.id)}"
      aria-pressed="${server.id === selectedId ? 'true' : 'false'}"
    >
      <div class="server-card-main">
        <h4 class="server-card-name">${escapeHtml(server.name)}</h4>
        <p class="server-card-host mono">${escapeHtml(server.host)}:${server.port}</p>
      </div>
      <div class="server-card-stats">
        <div class="server-stat">
          <span class="server-stat-label">Слоты</span>
          <span class="server-stat-value" data-field="slots">—</span>
        </div>
        <div class="server-stat">
          <span class="server-stat-label">Пинг</span>
          <span class="server-stat-value" data-field="ping">…</span>
        </div>
      </div>
      <span class="server-card-status" data-field="status">проверка</span>
    </button>
  `).join('');
}

function setupServerListEvents() {
  if (!els.serverList || els.serverList.dataset.bound === '1') return;
  els.serverList.dataset.bound = '1';
  els.serverList.addEventListener('click', (event) => {
    const card = event.target.closest('.server-card');
    if (!card) return;
    const serverId = card.getAttribute('data-server-id');
    const game = selectedGame();
    if (!game || !serverId) return;
    selectServer(game.id, serverId);
  });
}

function setServerPingLoading(key) {
  const card = els.serverList?.querySelector(`[data-server-key="${key}"]`);
  if (!card) return;

  card.classList.remove('server-card--online', 'server-card--offline');
  card.classList.add('server-card--pending');

  const stats = card.querySelector('.server-card-stats');
  const status = card.querySelector('[data-field="status"]');
  if (stats) stats.hidden = true;
  if (status) status.textContent = 'проверка';
}

function updateServerPing(key, result) {
  const card = els.serverList?.querySelector(`[data-server-key="${key}"]`);
  if (!card) return;

  const slots = card.querySelector('[data-field="slots"]');
  const ping = card.querySelector('[data-field="ping"]');
  const status = card.querySelector('[data-field="status"]');

  card.classList.remove('server-card--pending');

  if (result.online) {
    card.classList.remove('server-card--offline');
    card.classList.add('server-card--online');
    const stats = card.querySelector('.server-card-stats');
    if (stats) stats.hidden = false;
    if (slots) slots.textContent = formatSlots(result.playersOnline, result.playersMax);
    if (ping) ping.textContent = `${result.latencyMs} ms`;
    if (status) status.textContent = 'online';
  } else {
    card.classList.remove('server-card--online');
    card.classList.add('server-card--offline');
    const stats = card.querySelector('.server-card-stats');
    if (stats) stats.hidden = true;
    if (status) status.textContent = 'offline';
  }
}

async function pingSelectedServers() {
  const game = selectedGame();
  if (!game?.servers.length) return;

  for (const server of game.servers) {
    setServerPingLoading(server.key);
  }

  await Promise.all(game.servers.map(async server => {
    try {
      const result = await invoke('ping_server', { host: server.host, port: server.port });
      updateServerPing(server.key, result);
    } catch {
      updateServerPing(server.key, { online: false, playersOnline: 0, playersMax: 0, latencyMs: 0 });
    }
  }));
}

function schedulePing() {
  if (pingTimer) clearInterval(pingTimer);

  const game = selectedGame();
  if (!game?.playable || !game.servers.length) return;

  void pingSelectedServers();
  pingTimer = setInterval(() => void pingSelectedServers(), 20000);
}

function applyLauncherVersion(version) {
  if (!els.launcherVersion) return;
  els.launcherVersion.textContent = String(version || APP_VERSION).trim();
}

function applyBootstrap(data) {
  bootstrap = data;
  applyLauncherVersion(data.appVersion || APP_VERSION);
  gameCatalog = buildGameCatalog(data);

  if (!gameCatalog.some(g => g.id === selectedGameId && g.playable)) {
    const firstPlayable = gameCatalog.find(g => g.playable);
    selectedGameId = firstPlayable?.id || gameCatalog[0]?.id || 'minecraft';
  }

  const activeGame = selectedGame();
  if (activeGame) ensureServerSelection(activeGame);

  if (!data.enabled) {
    renderGameGrid('Лаунчер отключён администратором');
    els.serverPanel.hidden = true;
    return;
  }

  renderGameGrid();
  renderServerPanel();
  schedulePing();
}

async function refreshBootstrap() {
  try {
    const cached = await invoke('load_cached_bootstrap');
    if (cached) applyBootstrap(cached);
  } catch {
    // ignore cache read errors
  }

  try {
    const data = await fetchBootstrap();
    applyBootstrap(data);
    await invoke('cache_bootstrap', { payload: data });
  } catch {
    if (!bootstrap) {
      gameCatalog = buildGameCatalog({ enabled: false, games: [] });
      renderGameGrid('Нет связи с API');
      els.serverPanel.hidden = true;
    }
  } finally {
    await refreshClientPackUpdateState();
    updatePlayState();
  }
}

async function refreshClientPackUpdateState() {
  clientPackNeedsUpdate = false;
  if (!bootstrap?.enabled) return;
  if (selectedGameId !== 'minecraft') return;

  try {
    const minecraftVersion = bootstrap?.minecraftVersion;
    const clientPack = bootstrap?.clientPack ?? null;
    if (!minecraftVersion || !clientPack) return;

    clientPackNeedsUpdate = Boolean(await invoke('client_pack_update_needed', {
      gameId: selectedGameId,
      minecraftVersion,
      clientPack,
    }));
  } catch {
    clientPackNeedsUpdate = false;
  }
}

async function handlePlay() {
  if (busy || !canLaunchSelectedGame()) return;

  const nickname = els.nickname.value.trim();
  const launchNick = minecraftLaunchUsername(nickname);
  if (!validNickname(nickname) || launchNick.length < 3) return;

  busy = true;
  updatePlayState();
  setProgress(true, 0, 'Подготовка…');

  try {
    await saveProfile();
    const server = getSelectedServer();
    const result = await invoke('prepare_and_launch', {
      nickname: launchNick,
      gameId: selectedGameId,
      serverId: server?.id || null,
      bootstrap,
    });

    if (result?.launched) {
      setProgress(false);
      await refreshClientPackUpdateState();
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

function openSettings() {
  if (!els.settingsModal) return;
  els.settingsModal.hidden = false;
  els.settingsModal.setAttribute('aria-hidden', 'false');
  applyDisplayModeToUi(profile?.displayMode || DISPLAY_MODE_WINDOWED);
  void renderInstallPaths();
}

function shortenPath(path) {
  const value = String(path || '');
  if (value.length <= 52) return value;
  return `…${value.slice(-49)}`;
}

async function renderInstallPaths() {
  if (!els.installPathsList) return;

  const games = gameCatalog.length
    ? gameCatalog
    : [{ id: 'minecraft', name: 'Minecraft', playable: true }];

  const rows = await Promise.all(games.map(async (game) => {
    try {
      const info = await invoke('get_game_install_path', { gameId: game.id });
      return { game, info };
    } catch {
      return { game, info: null };
    }
  }));

  els.installPathsList.innerHTML = rows.map(({ game, info }) => {
    const disabled = !game.playable;
    const pathLabel = info?.isCustom ? 'Своя папка' : 'По умолчанию';
    const pathValue = info?.path || '—';

    return `
      <article class="install-path-row${disabled ? ' install-path-row--disabled' : ''}" data-game-id="${escapeHtml(game.id)}">
        <div class="install-path-head">
          <span class="install-path-game">${escapeHtml(game.name)}</span>
          <span class="install-path-badge">${escapeHtml(pathLabel)}</span>
        </div>
        <p class="install-path-value mono" title="${escapeHtml(pathValue)}">${escapeHtml(shortenPath(pathValue))}</p>
        <div class="install-path-actions">
          <button type="button" class="btn btn-secondary btn-compact install-path-pick" ${disabled ? 'disabled' : ''}>Выбрать…</button>
          <button type="button" class="btn btn-secondary btn-compact install-path-reset" ${disabled || !info?.isCustom ? 'disabled' : ''}>По умолчанию</button>
        </div>
      </article>`;
  }).join('');

  els.installPathsList.querySelectorAll('.install-path-pick').forEach(btn => {
    btn.addEventListener('click', async () => {
      const row = btn.closest('.install-path-row');
      const gameId = row?.getAttribute('data-game-id');
      if (!gameId) return;
      await pickInstallFolder(gameId);
    });
  });

  els.installPathsList.querySelectorAll('.install-path-reset').forEach(btn => {
    btn.addEventListener('click', async () => {
      const row = btn.closest('.install-path-row');
      const gameId = row?.getAttribute('data-game-id');
      if (!gameId) return;
      await resetInstallFolder(gameId);
    });
  });
}

async function pickInstallFolder(gameId) {
  if (busy || gameRunning) return;

  const game = gameCatalog.find(item => item.id === gameId);
  const info = await invoke('get_game_install_path', { gameId });

  let selected = null;
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    selected = await open({
      directory: true,
      multiple: false,
      title: `Папка установки — ${game?.name || gameId}`,
      defaultPath: info?.path || undefined,
    });
  } catch (err) {
    setProgress(true, 0, String(err || 'Не удалось открыть выбор папки'));
    setTimeout(() => setProgress(false), 5000);
    return;
  }

  if (!selected || Array.isArray(selected)) return;

  try {
    await invoke('set_game_install_path_cmd', { gameId, path: selected, bootstrap });
    if (profile) {
      profile.gameInstallPaths = { ...(profile.gameInstallPaths || {}), [gameId]: selected };
    }
    await renderInstallPaths();
  } catch (err) {
    setProgress(true, 0, String(err || 'Не удалось сохранить папку'));
    setTimeout(() => setProgress(false), 5000);
  }
}

async function resetInstallFolder(gameId) {
  if (busy || gameRunning) return;

  try {
    await invoke('set_game_install_path_cmd', { gameId, path: null, bootstrap });
    if (profile?.gameInstallPaths) {
      delete profile.gameInstallPaths[gameId];
    }
    await renderInstallPaths();
  } catch (err) {
    setProgress(true, 0, String(err || 'Не удалось сбросить папку'));
    setTimeout(() => setProgress(false), 5000);
  }
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

async function handleClearData() {
  if (busy || gameRunning) {
    setProgress(true, 0, gameRunning ? 'Закройте игру перед очисткой' : 'Дождитесь завершения операции');
    setTimeout(() => setProgress(false), 4000);
    return;
  }

  const confirmed = window.confirm(
    'Удалить все данные лаунчера?\n\nБудут удалены Java, клиенты (включая пользовательские папки), кэш и настройки. При следующем запуске всё скачается заново.',
  );
  if (!confirmed) return;

  const preservedNickname = els.nickname.value.trim();

  busy = true;
  updatePlayState();
  if (els.clearDataBtn) els.clearDataBtn.disabled = true;
  setProgress(true, 0, 'Очистка данных…');

  try {
    await invoke('clear_all_data');
    bootstrap = null;
    gameCatalog = [];
    profile = null;
    selectedServerKeys = {};
    clientPackNeedsUpdate = false;
    els.nickname.value = preservedNickname;
    applyDisplayModeToUi(DISPLAY_MODE_WINDOWED);
    selectedGameId = 'minecraft';
    await saveProfile();
    await closeSettings(false);
    setProgress(true, 100, 'Данные удалены');
    await refreshBootstrap();
    setTimeout(() => setProgress(false), 2500);
  } catch (err) {
    const message = String(err || 'Не удалось очистить данные');
    setProgress(true, 0, message);
    setTimeout(() => setProgress(false), 6000);
  } finally {
    busy = false;
    if (els.clearDataBtn) els.clearDataBtn.disabled = false;
    updatePlayState();
  }
}

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

els.nickname.addEventListener('input', () => {
  updatePlayState();
});
els.nickname.addEventListener('keydown', e => {
  if (e.key === 'Enter') handlePlay();
});
els.playBtn.addEventListener('click', handlePlay);
els.settingsBtn.addEventListener('click', () => openSettings());
els.settingsBackdrop?.addEventListener('click', () => closeSettings());
els.settingsCloseBtn?.addEventListener('click', () => closeSettings());
els.settingsSaveBtn?.addEventListener('click', () => closeSettings());
els.clearDataBtn?.addEventListener('click', () => handleClearData());
document.addEventListener('keydown', e => {
  if (e.key === 'Escape' && els.settingsModal && !els.settingsModal.hidden) {
    closeSettings();
  }
});

async function init() {
  await setupWindowControls();
  setupServerListEvents();

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
    updatePlayState();
  } catch {
    // ignore profile load errors
  }

  applyLauncherVersion(APP_VERSION);
  await refreshBootstrap();
}

init().catch(() => {
  renderGameGrid('Ошибка запуска');
});
