import { invoke } from '@tauri-apps/api/core';
import appVersionData from '../config/version.json';

document.addEventListener('contextmenu', (event) => {
  event.preventDefault();
}, { capture: true });

const APP_VERSION = appVersionData.version || '3.2.2 Creeper';
const DEFAULT_API = 'https://chadow.ru/api/minecraft/bootstrap';
const MINECRAFT_NICK_MAX = 16;

const PLAY_LABEL_IDLE = 'Играть';
const PLAY_LABEL_INSTALL = 'Установить';
const PLAY_LABEL_UPDATE = 'Обновить';
const PLAY_LABEL_RUNNING = 'Запущено';
const DISPLAY_MODE_WINDOWED = 'windowed';
const DISPLAY_MODE_FULLSCREEN = 'fullscreen';

/** Games shown before API support exists */
const PLACEHOLDER_GAMES = [
  {
    id: 'unknown',
    name: 'Неизвестно',
    subtitle: 'Данные отсутствуют',
    playable: false,
    accent: '#6b7280',
  },
];

const SERVER_ICON_PRESETS = {
  pickaxe: '⛏',
  sword: '⚔',
  castle: '🏰',
  globe: '🌐',
  fire: '🔥',
  star: '⭐',
  diamond: '💎',
  tree: '🌲',
  ship: '⛵',
  pick: '🪓',
};

const GAME_VISUALS = {
  minecraft: {
    subtitle: 'Java Edition',
    accent: '#74f6c8',
    glyph: '⛏',
  },
  unknown: {
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
  serverCarousel: document.getElementById('serverCarousel'),
  serverList: document.getElementById('serverList'),
  serverCarouselPrev: document.getElementById('serverCarouselPrev'),
  serverCarouselNext: document.getElementById('serverCarouselNext'),
  statusHint: document.getElementById('statusHint'),
  progressWrap: document.getElementById('progressWrap'),
  progressFill: document.getElementById('progressFill'),
  playBtnGroup: document.getElementById('playBtnGroup'),
  playBtn: document.getElementById('playBtn'),
  playBtnLabel: document.getElementById('playBtnLabel'),
  cancelBtn: document.getElementById('cancelBtn'),
  settingsBtn: document.getElementById('settingsBtn'),
  settingsModal: document.getElementById('settingsModal'),
  settingsBackdrop: document.getElementById('settingsBackdrop'),
  settingsCloseBtn: document.getElementById('settingsCloseBtn'),
  settingsSaveBtn: document.getElementById('settingsSaveBtn'),
  clearDataBtn: document.getElementById('clearDataBtn'),
  devModeCheckbox: document.getElementById('devModeCheckbox'),
  installPathsList: document.getElementById('installPathsList'),
  launcherVersion: document.getElementById('launcherVersion'),
  minimizeBtn: document.getElementById('minimizeBtn'),
  closeBtn: document.getElementById('closeBtn'),
};

/** @type {Record<string, unknown>|null} */
let bootstrap = null;
/** @type {{ nickname: string, apiUrl: string, gameInstallPaths?: Record<string, string>, selectedServers?: Record<string, string> }|null} */
let profile = null;
let launcherBusy = false;
let gameRunning = false;
/** @type {{ gameId: string, percent: number, message: string } | null} */
let installJob = null;
/** @type {{ percent: number, message: string } | null} */
let transientProgress = null;

/** @type {Array<{ id: string, name: string, subtitle: string, playable: boolean, badge?: string, accent: string, glyph: string, servers: Array<{ key: string, name: string, host: string, port: number }> }>} */
let gameCatalog = [];
let selectedGameId = 'minecraft';
/** @type {Record<string, string>} */
let selectedServerKeys = {};
/** @type {Record<string, { installed: boolean, needsUpdate: boolean }>} */
const clientStatusByGame = {};

function getClientStatus(gameId = selectedGameId) {
  return clientStatusByGame[gameId] ?? { installed: false, needsUpdate: false };
}

function isInstallRunning() {
  return installJob !== null;
}

function isSelectedGameInstalling() {
  return installJob?.gameId === selectedGameId;
}

function computePlayButtonLabel() {
  if (gameRunning) return PLAY_LABEL_RUNNING;
  const { installed, needsUpdate } = getClientStatus(selectedGameId);
  if (!installed) return PLAY_LABEL_INSTALL;
  return needsUpdate ? PLAY_LABEL_UPDATE : PLAY_LABEL_IDLE;
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

function getDevMode() {
  return Boolean(els.devModeCheckbox?.checked);
}

function applyDevModeToUi(enabled) {
  if (els.devModeCheckbox) {
    els.devModeCheckbox.checked = Boolean(enabled);
  }
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function resolveServerIcon(icon) {
  const raw = String(icon || '').trim();
  if (!raw) return null;
  if (SERVER_ICON_PRESETS[raw]) return { type: 'glyph', value: SERVER_ICON_PRESETS[raw] };
  if (/^https?:\/\//i.test(raw) || raw.startsWith('/')) {
    const url = raw.startsWith('/') ? `https://chadow.ru${raw}` : raw;
    return { type: 'image', value: url };
  }
  return { type: 'glyph', value: raw };
}

function renderServerIconMarkup(icon, accent) {
  const resolved = resolveServerIcon(icon);
  if (!resolved) {
    return `<span class="server-card-icon server-card-icon--fallback" aria-hidden="true">🌐</span>`;
  }
  if (resolved.type === 'image') {
    return `<span class="server-card-icon server-card-icon--image" aria-hidden="true"><img src="${escapeHtml(resolved.value)}" alt="" /></span>`;
  }
  return `<span class="server-card-icon" style="--server-accent:${escapeHtml(accent)}" aria-hidden="true">${escapeHtml(resolved.value)}</span>`;
}

function serverDescriptionLines(description) {
  if (Array.isArray(description)) {
    return description
      .map(line => String(line || '').trim())
      .filter(Boolean)
      .slice(0, 3);
  }

  return String(description || '')
    .split(/\r?\n/)
    .map(line => line.trim())
    .filter(Boolean)
    .slice(0, 3);
}

function renderServerDescriptionMarkup(description) {
  const lines = serverDescriptionLines(description);
  if (!lines.length) return '';

  return `<span class="server-card-desc">${lines
    .map(line => `<span class="server-card-desc-line">${escapeHtml(line)}</span>`)
    .join('')}</span>`;
}

function applyProgressBar(percent, message) {
  const pct = Math.max(0, Math.min(100, Math.round(percent)));
  const text = message || 'Подготовка…';

  if (els.progressFill) els.progressFill.style.width = `${pct}%`;
  if (els.statusHint) {
    els.statusHint.classList.toggle('status-hint--loading', true);
    els.statusHint.textContent = `${text} · ${pct}%`;
  }

  const bar = els.progressWrap?.querySelector('.progress-strip-bar');
  if (bar) {
    bar.setAttribute('aria-valuenow', String(pct));
    bar.setAttribute('aria-valuetext', `${pct}% — ${text}`);
  }
}

function setInstallProgress(percent, message) {
  if (!installJob) return;
  installJob.percent = percent;
  installJob.message = message || 'Подготовка…';
  syncFooterUi();
}

function showTransientProgress(percent, message, autoHideMs = 0) {
  transientProgress = { percent, message: message || 'Подготовка…' };
  syncFooterUi();
  if (autoHideMs > 0) {
    window.setTimeout(() => {
      transientProgress = null;
      syncFooterUi();
    }, autoHideMs);
  }
}

function syncFooterUi() {
  const showInstall = isSelectedGameInstalling();
  const showTransient = transientProgress !== null && !showInstall;
  const showProgress = showInstall || showTransient;

  if (els.progressWrap) {
    els.progressWrap.hidden = !showProgress;
  }

  if (showInstall && installJob) {
    applyProgressBar(installJob.percent, installJob.message);
  } else if (showTransient && transientProgress) {
    applyProgressBar(transientProgress.percent, transientProgress.message);
  } else if (els.statusHint) {
    els.statusHint.classList.remove('status-hint--loading');
    updateStatusHint();
  }

  setCancelVisible(showInstall);
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

    let check = card.querySelector('.server-card-check');
    if (isSelected && !check) {
      check = document.createElement('span');
      check.className = 'server-card-check';
      check.setAttribute('aria-hidden', 'true');
      check.textContent = '✓';
      card.appendChild(check);
    } else if (!isSelected && check) {
      check.remove();
    }
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

function setCancelVisible(visible) {
  els.playBtnGroup?.classList.toggle('btn-play-group--installing', visible);
}

function updateStatusHint() {
  if (!els.statusHint) return;

  if (isSelectedGameInstalling()) return;

  if (gameRunning) {
    els.statusHint.textContent = 'Игра запущена — закройте клиент, чтобы сыграть снова';
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

  if (game.id === 'minecraft' && !server) {
    els.statusHint.textContent = `Выберите сервер — ${game.name}`;
    return;
  }

  const gameLine = `${game.name}${server ? ` · ${server.name}` : ''}`;
  const { installed, needsUpdate } = getClientStatus(game.id);

  if (game.id === 'minecraft' && !installed) {
    els.statusHint.textContent = `Требуется установка — ${gameLine}`;
    return;
  }

  if (game.id === 'minecraft' && needsUpdate) {
    els.statusHint.textContent = `Доступно обновление — ${gameLine}`;
    return;
  }

  els.statusHint.textContent = `Готово к запуску — ${gameLine}`;
}

function updatePlayState() {
  updateNicknameWarning();
  syncFooterUi();

  if (els.playBtnLabel) {
    els.playBtnLabel.textContent = computePlayButtonLabel();
  }

  if (gameRunning) {
    els.playBtn.disabled = true;
    return;
  }

  const nick = els.nickname.value.trim();
  const installBlocksPlay = isSelectedGameInstalling();
  const ready = !installBlocksPlay && !launcherBusy && canLaunchSelectedGame() && validNickname(nick);
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
  applyDevModeToUi(profile?.devMode);
}

async function saveProfile() {
  profile = {
    nickname: els.nickname.value.trim(),
    apiUrl: DEFAULT_API,
    gameInstallPaths: profile?.gameInstallPaths || {},
    selectedServers: { ...selectedServerKeys },
    displayMode: getDisplayMode(),
    devMode: getDevMode(),
  };
  await invoke('save_profile', { profile });
}

async function fetchBootstrap() {
  return invoke('fetch_bootstrap', { apiUrl: getApiUrl() });
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
        icon: server.icon ? String(server.icon) : '',
        description: serverDescriptionLines(server.description),
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
    const disabled = !game.playable;
    return `
      <button
        type="button"
        class="game-card${selected ? ' game-card--selected' : ''}${disabled ? ' game-card--muted game-card--disabled' : ''}"
        data-game-id="${escapeHtml(game.id)}"
        data-playable="${disabled ? 'false' : 'true'}"
        role="option"
        aria-selected="${selected ? 'true' : 'false'}"
        aria-disabled="${disabled ? 'true' : 'false'}"
        ${disabled ? 'disabled tabindex="-1"' : ''}
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
      if (btn.disabled || btn.getAttribute('data-playable') === 'false') return;
      const id = btn.getAttribute('data-game-id');
      if (!id || id === selectedGameId) return;
      const game = gameCatalog.find(item => item.id === id);
      if (!game?.playable) return;
      selectedGameId = id;
      renderGameGrid();
      renderServerPanel();
      if (!isInstallRunning() || installJob.gameId !== id) {
        await refreshClientPackUpdateState(id);
      }
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

  els.serverList.innerHTML = game.servers.map(server => {
    const accent = game.accent || '#64b5f6';
    const selected = server.id === selectedId;
    return `
    <button
      type="button"
      class="server-card${selected ? ' server-card--selected' : ''}"
      data-server-key="${escapeHtml(server.key)}"
      data-server-id="${escapeHtml(server.id)}"
      aria-pressed="${selected ? 'true' : 'false'}"
      style="--server-accent: ${escapeHtml(accent)}"
    >
      <span class="server-card-glow" aria-hidden="true"></span>
      <span class="server-card-top">
        ${renderServerIconMarkup(server.icon, accent)}
        ${renderServerDescriptionMarkup(server.description)}
      </span>
      <span class="server-card-meta">
        <span class="server-card-name">${escapeHtml(server.name)}</span>
        <span class="server-card-host mono">${escapeHtml(server.host)}:${server.port}</span>
      </span>
      ${selected ? '<span class="server-card-check" aria-hidden="true">✓</span>' : ''}
    </button>`;
  }).join('');

  updateServerCarouselNav();
}

function updateServerCarouselNav() {
  const track = els.serverList;
  const nav = els.serverCarousel;
  if (!track || !nav) return;

  const overflow = track.scrollWidth > track.clientWidth + 4;
  nav.classList.toggle('server-carousel--scrollable', overflow);

  if (els.serverCarouselPrev) {
    els.serverCarouselPrev.hidden = !overflow;
    els.serverCarouselPrev.disabled = !overflow || track.scrollLeft <= 4;
  }
  if (els.serverCarouselNext) {
    els.serverCarouselNext.hidden = !overflow;
    els.serverCarouselNext.disabled = !overflow || track.scrollLeft + track.clientWidth >= track.scrollWidth - 4;
  }
}

function scrollServerCarousel(direction) {
  const track = els.serverList;
  if (!track) return;
  const amount = Math.max(220, Math.round(track.clientWidth * 0.72));
  track.scrollBy({ left: direction * amount, behavior: 'smooth' });
  window.setTimeout(updateServerCarouselNav, 280);
}

function setupServerCarousel() {
  if (!els.serverList || els.serverList.dataset.carouselBound === '1') return;
  els.serverList.dataset.carouselBound = '1';

  els.serverList.addEventListener('scroll', () => {
    window.requestAnimationFrame(updateServerCarouselNav);
  }, { passive: true });

  els.serverCarouselPrev?.addEventListener('click', () => scrollServerCarousel(-1));
  els.serverCarouselNext?.addEventListener('click', () => scrollServerCarousel(1));
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

async function refreshClientPackUpdateState(gameId = selectedGameId) {
  if (isInstallRunning() && installJob.gameId === gameId) return;

  clientStatusByGame[gameId] = { installed: false, needsUpdate: false };
  if (!bootstrap?.enabled) return;
  if (gameId !== 'minecraft') return;

  try {
    const minecraftVersion = bootstrap?.minecraftVersion;
    const clientPack = bootstrap?.clientPack ?? null;
    if (!minecraftVersion) return;

    const status = await invoke('client_install_status', {
      gameId,
      minecraftVersion,
      clientPack,
    });

    clientStatusByGame[gameId] = {
      installed: Boolean(status?.installed),
      needsUpdate: Boolean(status?.needsUpdate),
    };
  } catch {
    clientStatusByGame[gameId] = { installed: false, needsUpdate: false };
  }
}

async function handlePlay() {
  if (isSelectedGameInstalling() || launcherBusy || !canLaunchSelectedGame()) return;

  const nickname = els.nickname.value.trim();
  const launchNick = minecraftLaunchUsername(nickname);
  if (!validNickname(nickname) || launchNick.length < 3) return;

  const gameId = selectedGameId;
  installJob = { gameId, percent: 0, message: 'Подготовка…' };
  updatePlayState();

  try {
    await saveProfile();
    const server = getSelectedServer();
    const result = await invoke('prepare_and_launch', {
      nickname: launchNick,
      gameId,
      serverId: server?.id || null,
      bootstrap,
    });

    installJob = null;

    if (result?.launched) {
      await refreshClientPackUpdateState(gameId);
      setPlayButtonRunning(true);
      return;
    }
  } catch (err) {
    installJob = null;
    const message = String(err || 'Не удалось запустить игру');
    const lowered = message.toLowerCase();
    const isCancelled = lowered.includes('отмен');
    showTransientProgress(0, isCancelled ? 'Установка отменена' : message, isCancelled ? 2000 : 12000);
  }

  await refreshClientPackUpdateState(gameId);
  updatePlayState();
}

async function handleCancelInstall() {
  if (!isInstallRunning()) return;
  try {
    await invoke('cancel_install');
  } catch {
    // ignore cancel errors
  }
}

function openSettings() {
  if (!els.settingsModal) return;
  els.settingsModal.hidden = false;
  els.settingsModal.setAttribute('aria-hidden', 'false');
  applyDisplayModeToUi(profile?.displayMode || DISPLAY_MODE_WINDOWED);
  applyDevModeToUi(profile?.devMode);
  void renderInstallPaths();
}

function shortenPath(path) {
  const value = String(path || '');
  if (value.length <= 52) return value;
  return `…${value.slice(-49)}`;
}

async function renderInstallPaths() {
  if (!els.installPathsList) return;

  const games = gameCatalog.filter(game => game.playable);
  const list = games.length
    ? games
    : [{ id: 'minecraft', name: 'Minecraft', playable: true }];

  const rows = await Promise.all(list.map(async (game) => {
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
  if (isInstallRunning() || launcherBusy || gameRunning) return;

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
    showTransientProgress(0, String(err || 'Не удалось открыть выбор папки'), 5000);
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
    showTransientProgress(0, String(err || 'Не удалось сохранить папку'), 5000);
  }
}

async function resetInstallFolder(gameId) {
  if (isInstallRunning() || launcherBusy || gameRunning) return;

  try {
    await invoke('set_game_install_path_cmd', { gameId, path: null, bootstrap });
    if (profile?.gameInstallPaths) {
      delete profile.gameInstallPaths[gameId];
    }
    await renderInstallPaths();
  } catch (err) {
    showTransientProgress(0, String(err || 'Не удалось сбросить папку'), 5000);
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
  if (isInstallRunning() || launcherBusy || gameRunning) {
    showTransientProgress(
      0,
      gameRunning ? 'Закройте игру перед очисткой' : 'Дождитесь завершения операции',
      4000,
    );
    return;
  }

  const confirmed = window.confirm(
    'Удалить все данные лаунчера?\n\nБудут удалены Java, клиенты (включая пользовательские папки), кэш и настройки. При следующем запуске всё скачается заново.',
  );
  if (!confirmed) return;

  const preservedNickname = els.nickname.value.trim();

  launcherBusy = true;
  updatePlayState();
  if (els.clearDataBtn) els.clearDataBtn.disabled = true;
  showTransientProgress(0, 'Очистка данных…');

  try {
    await invoke('clear_all_data');
    bootstrap = null;
    gameCatalog = [];
    profile = null;
    selectedServerKeys = {};
    installJob = null;
    Object.keys(clientStatusByGame).forEach((key) => {
      delete clientStatusByGame[key];
    });
    els.nickname.value = preservedNickname;
    applyDisplayModeToUi(DISPLAY_MODE_WINDOWED);
    selectedGameId = 'minecraft';
    await saveProfile();
    await closeSettings(false);
    showTransientProgress(100, 'Данные удалены', 2500);
    await refreshBootstrap();
  } catch (err) {
    const message = String(err || 'Не удалось очистить данные');
    showTransientProgress(0, message, 6000);
  } finally {
    launcherBusy = false;
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
els.cancelBtn?.addEventListener('click', () => handleCancelInstall());
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
  setupServerCarousel();

  try {
    const { listen } = await import('@tauri-apps/api/event');
    await listen('install-progress', ({ payload }) => {
      const p = /** @type {{ percent: number, message: string }} */ (payload);
      if (installJob) {
        setInstallProgress(p.percent, p.message);
      }
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
