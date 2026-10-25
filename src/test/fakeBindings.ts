import { vi } from "vitest";
import type { AppError, PlayResult, SettingsDto, SoundSummary, ThemeDto } from "../shared/ipc/bindings";

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
  playResult: "started" as PlayResult,
};

export function resetBackend(): void {
  backend.settings = { language: "system", masterGainMillibels: 0, theme: "system" };
  backend.sounds = [];
  backend.failUpdateWith = null;
  backend.holdUpdate = null;
  backend.failReadsWith = null;
  backend.appInfoFails = false;
  backend.playResult = "started";
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
  playSound: vi.fn(async (_id: number) => ok(backend.playResult)),
  stopAllSounds: vi.fn(async () => ok(null)),
  retryAudio: vi.fn(async () => ok(null)),
  importSound: vi.fn(async () => ok(null)),
  setSoundOptions: vi.fn(async () => failed({ kind: "internal" })),
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
