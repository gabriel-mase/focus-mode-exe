import { useState, useEffect, useRef } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ActionIcon,
  Badge,
  Box,
  Button,
  Checkbox,
  Flex,
  Group,
  Popover,
  ScrollArea,
  Stack,
  Text,
  TextInput,
  Tooltip,
} from "@mantine/core";
import {
  IconBolt,
  IconDeviceDesktop,
  IconFolderSearch,
  IconPlayerPlay,
  IconRefresh,
  IconTrash,
  IconX,
} from "@tabler/icons-react";

// ── Types ──────────────────────────────────────────────────────────────────────

interface Game {
  app_id: string;
  name: string;
  install_dir: string;
  exe_name: string | null;
  exe_path: string | null;
  is_custom: boolean;
}

interface MonitorInfo {
  device_name: string;
  friendly_name: string;
  width: number;
  height: number;
  refresh_hz: number;
  is_primary: boolean;
}

// ── Monitor config popover ─────────────────────────────────────────────────────

function MonitorConfigPanel({
  game,
  monitors,
  currentConfig,
  onSave,
}: {
  game: Game;
  monitors: MonitorInfo[];
  currentConfig: string[];
  onSave: (deviceNames: string[]) => void;
}) {
  const secondary = monitors.filter((m) => !m.is_primary);
  // Empty config = "all secondary" (default). Otherwise, specific device names.
  const isDefault = currentConfig.length === 0;
  const [useDefault, setUseDefault] = useState(isDefault);
  const [selected, setSelected] = useState<string[]>(
    isDefault ? secondary.map((m) => m.device_name) : currentConfig
  );

  function handleDefaultChange(checked: boolean) {
    setUseDefault(checked);
    if (checked) setSelected(secondary.map((m) => m.device_name));
  }

  function handleSave() {
    onSave(useDefault ? [] : selected);
  }

  if (secondary.length === 0) {
    return (
      <Box p="xs">
        <Text size="xs" c="dimmed">Only one monitor detected</Text>
      </Box>
    );
  }

  return (
    <Stack gap="xs" p={4} style={{ minWidth: 230 }}>
      <Text size="xs" fw={600} c="indigo.3">
        Monitors — {game.name}
      </Text>

      <Checkbox
        label={<Text size="xs">All secondary (default)</Text>}
        checked={useDefault}
        onChange={(e) => handleDefaultChange(e.currentTarget.checked)}
        size="xs"
      />

      {!useDefault && (
        <Stack gap={4}>
          <Text size="10px" c="dimmed" tt="uppercase" fw={500}>
            Disable on game start:
          </Text>
          {secondary.map((m) => (
            <Checkbox
              key={m.device_name}
              size="xs"
              label={
                <Text size="xs">
                  {m.friendly_name.split("\\").pop() || m.friendly_name}{" "}
                  <Text span size="10px" c="dimmed">
                    {m.width}×{m.height}
                  </Text>
                </Text>
              }
              checked={selected.includes(m.device_name)}
              onChange={(e) => {
                if (e.currentTarget.checked) {
                  setSelected((p) => [...p, m.device_name]);
                } else {
                  setSelected((p) => p.filter((d) => d !== m.device_name));
                }
              }}
            />
          ))}
        </Stack>
      )}

      <Button size="xs" variant="light" color="indigo" onClick={handleSave} mt={2}>
        Save
      </Button>
    </Stack>
  );
}

// ── Game row ───────────────────────────────────────────────────────────────────

function GameRow({
  game,
  isEnabled,
  monitors,
  monitorConfig,
  onToggle,
  onBrowse,
  onRemove,
  onMonitorConfigSave,
}: {
  game: Game;
  isEnabled: boolean;
  monitors: MonitorInfo[];
  monitorConfig: string[];
  onToggle: () => void;
  onBrowse: (e: React.MouseEvent) => void;
  onRemove: (e: React.MouseEvent) => void;
  onMonitorConfigSave: (deviceNames: string[]) => void;
}) {
  const [popOpen, setPopOpen] = useState(false);
  const hasExe = !!game.exe_name;
  const hasSecondaryMonitors = monitors.some((m) => !m.is_primary);
  const configuredCount =
    monitorConfig.length > 0
      ? monitorConfig.length
      : monitors.filter((m) => !m.is_primary).length;

  return (
    <Box
      px="sm"
      py={7}
      onClick={() => hasExe && onToggle()}
      style={{
        cursor: hasExe ? "pointer" : "default",
        borderBottom: "1px solid var(--mantine-color-dark-6)",
        borderLeft: isEnabled
          ? "2px solid var(--mantine-color-indigo-6)"
          : "2px solid transparent",
        background: isEnabled ? "rgba(79,91,196,.07)" : undefined,
        transition: "background 0.1s",
      }}
      className="game-row"
    >
      <Group gap="sm" wrap="nowrap">
        {/* Checkbox */}
        <Checkbox
          checked={isEnabled}
          readOnly
          size="xs"
          disabled={!hasExe}
          onClick={(e) => e.stopPropagation()}
          styles={{ input: { cursor: hasExe ? "pointer" : "default" } }}
        />

        {/* Name + exe */}
        <Box flex={1} style={{ minWidth: 0 }}>
          <Group gap={6} wrap="nowrap">
            <Text
              size="sm"
              fw={isEnabled ? 500 : 400}
              c={hasExe ? (isEnabled ? "white" : "gray.3") : "dark.3"}
              style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
            >
              {game.name}
            </Text>
            {game.is_custom && (
              <Badge size="xs" variant="outline" color="gray" style={{ flexShrink: 0 }}>
                custom
              </Badge>
            )}
          </Group>
          <Text
            size="xs"
            c={hasExe ? (isEnabled ? "dark.2" : "dark.3") : "dark.5"}
            style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}
            fs={!hasExe ? "italic" : undefined}
          >
            {game.exe_name ?? "No executable detected"}
          </Text>
        </Box>

        {/* Action buttons — visible on hover via CSS */}
        <Group gap={4} wrap="nowrap" className="row-actions">
          {/* Monitor config — only when enabled and secondary monitors exist */}
          {hasExe && isEnabled && hasSecondaryMonitors && (
            <Popover
              opened={popOpen}
              onChange={setPopOpen}
              position="left-start"
              withArrow
              withinPortal
              shadow="md"
            >
              <Popover.Target>
                <Tooltip label={`${configuredCount} monitor(s) will be disabled`} position="left">
                  <ActionIcon
                    size="sm"
                    variant="subtle"
                    color="indigo"
                    onClick={(e) => {
                      e.stopPropagation();
                      setPopOpen((o) => !o);
                    }}
                  >
                    <IconDeviceDesktop size={13} />
                  </ActionIcon>
                </Tooltip>
              </Popover.Target>
              <Popover.Dropdown onClick={(e) => e.stopPropagation()}>
                <MonitorConfigPanel
                  game={game}
                  monitors={monitors}
                  currentConfig={monitorConfig}
                  onSave={(names) => {
                    onMonitorConfigSave(names);
                    setPopOpen(false);
                  }}
                />
              </Popover.Dropdown>
            </Popover>
          )}

          {/* Browse exe — Steam games */}
          {!game.is_custom && (
            <Tooltip label={hasExe ? "Change executable" : "Browse for executable"} position="left">
              <ActionIcon
                size="sm"
                variant={hasExe ? "subtle" : "light"}
                color={hasExe ? "gray" : "indigo"}
                onClick={(e) => {
                  e.stopPropagation();
                  onBrowse(e);
                }}
              >
                <IconFolderSearch size={13} />
              </ActionIcon>
            </Tooltip>
          )}

          {/* Remove — custom games */}
          {game.is_custom && (
            <Tooltip label="Remove game" position="left">
              <ActionIcon
                size="sm"
                variant="subtle"
                color="red"
                onClick={(e) => {
                  e.stopPropagation();
                  onRemove(e);
                }}
              >
                <IconTrash size={13} />
              </ActionIcon>
            </Tooltip>
          )}
        </Group>
      </Group>
    </Box>
  );
}

// ── Main app ───────────────────────────────────────────────────────────────────

export default function App() {
  const [games, setGames] = useState<Game[]>([]);
  const [enabledExes, setEnabledExes] = useState<Set<string>>(new Set());
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [monitorConfigs, setMonitorConfigs] = useState<Record<string, string[]>>({});
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [pendingExe, setPendingExe] = useState<{ path: string; name: string } | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    loadAll();
  }, []);

  useEffect(() => {
    if (pendingExe) nameRef.current?.focus();
  }, [pendingExe]);

  async function loadAll() {
    setLoading(true);
    try {
      const [gameList, enabledList, monitorList, configs] = await Promise.all([
        invoke<Game[]>("get_games"),
        invoke<string[]>("get_enabled_exes"),
        invoke<MonitorInfo[]>("get_monitors"),
        invoke<Record<string, string[]>>("get_game_monitor_configs"),
      ]);
      setGames(gameList);
      setEnabledExes(new Set(enabledList));
      setMonitors(monitorList);
      setMonitorConfigs(configs);
    } finally {
      setLoading(false);
    }
  }

  async function handleRefresh() {
    setRefreshing(true);
    try {
      const gameList = await invoke<Game[]>("refresh_games");
      setGames(gameList);
    } finally {
      setRefreshing(false);
    }
  }

  async function toggleGame(game: Game) {
    if (!game.exe_name) return;
    const newEnabled = !enabledExes.has(game.exe_name);
    await invoke("set_game_enabled", { exeName: game.exe_name, enabled: newEnabled });
    setEnabledExes((prev) => {
      const next = new Set(prev);
      newEnabled ? next.add(game.exe_name!) : next.delete(game.exe_name!);
      return next;
    });
  }

  async function browseForExe(game: Game) {
    const path = await invoke<string | null>("pick_exe_file");
    if (!path) return;
    await invoke("set_game_exe", { appId: game.app_id, exePath: path });
    setGames((prev) =>
      prev.map((g) =>
        g.app_id === game.app_id
          ? { ...g, exe_path: path, exe_name: path.split(/[\\/]/).pop() ?? path }
          : g
      )
    );
  }

  async function startAddCustomGame() {
    const path = await invoke<string | null>("pick_exe_file");
    if (!path) return;
    const parts = path.split(/[\\/]/);
    const suggested = parts.length >= 2 ? parts[parts.length - 2] : (parts.pop() ?? "");
    setPendingExe({ path, name: suggested });
  }

  async function confirmCustomGame(name: string) {
    if (!pendingExe || !name.trim()) return;
    const game = await invoke<Game>("add_custom_game", {
      name: name.trim(),
      exePath: pendingExe.path,
    });
    setGames((prev) =>
      [...prev, game].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()))
    );
    setPendingExe(null);
  }

  async function removeCustomGame(game: Game) {
    await invoke("remove_custom_game", { appId: game.app_id });
    if (game.exe_name) {
      setEnabledExes((prev) => { const n = new Set(prev); n.delete(game.exe_name!); return n; });
    }
    setGames((prev) => prev.filter((g) => g.app_id !== game.app_id));
  }

  async function saveMonitorConfig(game: Game, deviceNames: string[]) {
    if (!game.exe_name) return;
    await invoke("set_game_monitor_config", {
      exeName: game.exe_name,
      monitorDeviceNames: deviceNames,
    });
    setMonitorConfigs((prev) => ({ ...prev, [game.exe_name!]: deviceNames }));
  }

  function openGitHub() {
    if (isTauri()) openUrl("https://github.com/gabriel-mase").catch(() => {});
  }

  const filtered = games.filter((g) =>
    g.name.toLowerCase().includes(search.toLowerCase())
  );
  const enabledCount = games.filter((g) => g.exe_name && enabledExes.has(g.exe_name)).length;
  const secondaryMonitors = monitors.filter((m) => !m.is_primary);

  return (
    <Flex direction="column" h="100vh">
      {/* ── Header ──────────────────────────────────────────────────────── */}
      <Box
        px="md"
        py="xs"
        style={{ borderBottom: "1px solid var(--mantine-color-dark-5)", flexShrink: 0 }}
      >
        <Group justify="space-between" align="center">
          <Stack gap={0}>
            <Group gap={6}>
              <IconBolt size={16} color="var(--mantine-color-indigo-4)" />
              <Text size="sm" fw={700} c="indigo.3" style={{ letterSpacing: 0.4 }}>
                FOCUS MODE
              </Text>
              {enabledCount > 0 && (
                <Badge size="xs" variant="dot" color="indigo">
                  {enabledCount} active
                </Badge>
              )}
            </Group>
            <Text size="10px" c="dark.3" style={{ letterSpacing: 0.2 }}>
              Your game. Your screen. Your rules.
            </Text>
          </Stack>

          {secondaryMonitors.length > 0 && (
            <Stack gap={0} align="flex-end">
              <Text size="10px" c="dark.3">
                {secondaryMonitors.length} secondary monitor{secondaryMonitors.length > 1 ? "s" : ""}
              </Text>
              <Text size="10px" c="dark.4">
                {secondaryMonitors.map((m) => `${m.width}×${m.height}`).join(" · ")}
              </Text>
            </Stack>
          )}
        </Group>
      </Box>

      {/* ── Toolbar ─────────────────────────────────────────────────────── */}
      <Box
        px="sm"
        py={7}
        style={{ borderBottom: "1px solid var(--mantine-color-dark-6)", flexShrink: 0 }}
      >
        <Group gap="xs">
          <TextInput
            flex={1}
            size="xs"
            placeholder="Search games..."
            value={search}
            onChange={(e) => setSearch(e.currentTarget.value)}
            leftSection={null}
            styles={{ input: { background: "var(--mantine-color-dark-8)" } }}
          />
          <Tooltip label="Add non-Steam game">
            <Button
              size="xs"
              variant="light"
              color="indigo"
              leftSection={<IconPlayerPlay size={12} />}
              onClick={startAddCustomGame}
            >
              Add Game
            </Button>
          </Tooltip>
          <Tooltip label="Re-scan Steam library">
            <ActionIcon
              size="sm"
              variant="subtle"
              color="gray"
              onClick={handleRefresh}
              loading={refreshing}
            >
              <IconRefresh size={14} />
            </ActionIcon>
          </Tooltip>
        </Group>
      </Box>

      {/* ── Add custom game form ─────────────────────────────────────────── */}
      {pendingExe && (
        <Box
          px="sm"
          py={7}
          style={{ borderBottom: "1px solid var(--mantine-color-indigo-9)", flexShrink: 0, background: "rgba(79,91,196,.08)" }}
        >
          <Group gap="xs">
            <Text size="10px" c="dimmed" style={{ maxWidth: 110, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {pendingExe.path.split(/[\\/]/).pop()}
            </Text>
            <TextInput
              ref={nameRef}
              flex={1}
              size="xs"
              defaultValue={pendingExe.name}
              placeholder="Game name..."
              styles={{ input: { background: "var(--mantine-color-dark-8)" } }}
              onKeyDown={(e) => {
                if (e.key === "Enter") confirmCustomGame(e.currentTarget.value);
                if (e.key === "Escape") setPendingExe(null);
              }}
            />
            <Button
              size="xs"
              variant="filled"
              color="indigo"
              onClick={() => confirmCustomGame(nameRef.current?.value ?? "")}
            >
              Add
            </Button>
            <ActionIcon size="sm" variant="subtle" color="gray" onClick={() => setPendingExe(null)}>
              <IconX size={13} />
            </ActionIcon>
          </Group>
        </Box>
      )}

      {/* ── Game list ────────────────────────────────────────────────────── */}
      <ScrollArea flex={1} scrollbarSize={5}>
        {loading ? (
          <Flex h={200} align="center" justify="center">
            <Text size="sm" c="dark.4">Scanning Steam library...</Text>
          </Flex>
        ) : filtered.length === 0 ? (
          <Flex h={200} align="center" justify="center">
            <Text size="sm" c="dark.4">
              {search ? "No games match your search" : "No games detected"}
            </Text>
          </Flex>
        ) : (
          filtered.map((game) => (
            <GameRow
              key={game.app_id}
              game={game}
              isEnabled={!!game.exe_name && enabledExes.has(game.exe_name)}
              monitors={monitors}
              monitorConfig={game.exe_name ? (monitorConfigs[game.exe_name] ?? []) : []}
              onToggle={() => toggleGame(game)}
              onBrowse={() => browseForExe(game)}
              onRemove={() => removeCustomGame(game)}
              onMonitorConfigSave={(names) => saveMonitorConfig(game, names)}
            />
          ))
        )}
      </ScrollArea>

      {/* ── Footer ──────────────────────────────────────────────────────── */}
      <Box
        px="sm"
        py={5}
        style={{ borderTop: "1px solid var(--mantine-color-dark-6)", flexShrink: 0 }}
      >
        <Group justify="space-between" align="center">
          <Text size="10px" c="dark.4">
            {enabledCount > 0
              ? `Monitors switch automatically when a game starts`
              : `Enable games to activate automatic monitor switching`}
          </Text>
          <Group gap={6}>
            <Text size="10px" c="dark.5">by</Text>
            <Text
              size="10px"
              c="dark.3"
              style={{ cursor: "pointer", textDecoration: "underline dotted" }}
              onClick={openGitHub}
            >
              gabriel-mase
            </Text>
          </Group>
        </Group>
      </Box>

      {/* Hover style for game rows */}
      <style>{`
        .game-row:hover { background: rgba(255,255,255,0.03) !important; }
        .game-row .row-actions { opacity: 0; transition: opacity 0.15s; }
        .game-row:hover .row-actions { opacity: 1; }
      `}</style>
    </Flex>
  );
}
