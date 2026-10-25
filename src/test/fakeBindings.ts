import { vi } from "vitest";
import type { AppError, ImportResult, PlayModeDto, PlayResult, SettingsDto, SoundSummary, ThemeDto } from "../shared/ipc/bindings";

/** The fake backend behind the IPC boundary. It follows the real contract: results are `{status, data|error}`. */
export const backend = {
  settings: { language: "system", masterGainMillibels: 0, theme: "system" } as SettingsDto,
  sounds: [] as SoundSummary[],
  /** When set, the next `updateSettings` fails with this error. */
  failUpdateWith: null as AppError | null,
  /** When set, `updateSettings` waits for this promise before answering. */
  holdUpdate: null as Promise<void> | null,
  /** When set, `getSettings`/`listSounds` fail with this error. */
  failReadsWith: null as AppError | null,
  appInfoFails: false,
  /** What `importSound` answers: `null` is a cancelled dialog. */
  importResult: null as ImportResult | null,
  importError: null as AppError | null,
  playing: [] as number[],
  playResult: "started" as PlayResult,
};

export function resetBackend(): void {
  backend.settings = { language: "system", masterGainMillibels: 0, theme: "system" };
  backend.sounds = [];
  backend.failUpdateWith = null;
  backend.holdUpdate = null;
  backend.failReadsWith = null;
  backend.appInfoFails = false;
  backend.importResult = null;
  backend.importError = null;
  backend.playing = [];
  backend.playResult = "started";
}

type PlayingEvent = { payload: { ids: number[] } };
const playingListeners = new Set<(event: PlayingEvent) => void>();

/** The `PlayingChanged` event of the real bindings. `emitPlaying` is the backend announcing a change. */
export const events = {
  playingChanged: {
    listen: vi.fn(async (handler: (event: PlayingEvent) => void) => {
      playingListeners.add(handler);
      return () => {
        playingListeners.delete(handler);
      };
    }),
  },
};

export function emitPlaying(ids: number[]): void {
  backend.playing = [...ids];
  for (const listener of playingListeners) listener({ payload: { ids } });
}

const ok = <T>(data: T) => ({ status: "ok" as const, data });
const failed = (error: AppError) => ({ status: "error" as const, error });

export const commands = {
  appInfo: vi.fn(async () => {
    if (backend.appInfoFails) throw new Error("unreachable");
    return { name: "ScreamPad", version: "0.1.0" };
  }),
  getSettings: vi.fn(async () => (backend.failReadsWith ? failed(backend.failReadsWith) : ok({ ...backend.settings }))),
  updateSettings: vi.fn(async (language: string, masterGainMillibels: number, theme: ThemeDto) => {
    if (backend.holdUpdate) await backend.holdUpdate;
    if (backend.failUpdateWith) return failed(backend.failUpdateWith);
    backend.settings = { language, masterGainMillibels, theme };
    return ok({ ...backend.settings });
  }),
  listSounds: vi.fn(async () => (backend.failReadsWith ? failed(backend.failReadsWith) : ok([...backend.sounds]))),
  audioStatus: vi.fn(async () => ok({ state: "running" as const, failure: null, sampleRate: 48000, channels: 2 })),
  playSound: vi.fn(async (_id: number): Promise<{ status: "ok"; data: PlayResult } | { status: "error"; error: AppError }> =>
    ok(backend.playResult),
  ),
  stopAllSounds: vi.fn(async () => ok(null)),
  stopSound: vi.fn(async (_id: number) => ok(null)),
  playingSounds: vi.fn(async () => ok([...backend.playing])),
  retryAudio: vi.fn(async () => ok(null)),
  importSound: vi.fn(async () => (backend.importError ? failed(backend.importError) : ok(backend.importResult))),
  setSoundOptions: vi.fn(async (id: number, gainMillibels: number, playMode: PlayModeDto) => {
    const found = backend.sounds.find((candidate) => candidate.id === id);
    if (!found) return failed({ kind: "soundNotFound" });
    const updated = { ...found, gainMillibels, playMode };
    backend.sounds = backend.sounds.map((candidate) => (candidate.id === id ? updated : candidate));
    return ok(updated);
  }),
};

export function sound(overrides: Partial<SoundSummary> & { id: number }): SoundSummary {
  return {
    displayName: `Sound ${overrides.id}`,
    format: "wav",
    durationMs: 1500,
    sampleRate: 48000,
    channels: 2,
    sizeBytes: 1000,
    status: "available",
    gainMillibels: 0,
    playMode: "overlap",
    ...overrides,
  };
}
