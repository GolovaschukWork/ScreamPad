import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Button } from "@/shared/ui/button";
import { ChoiceSelect } from "@/shared/components/ChoiceSelect";
import { commands, type AudioDeviceDto, type RoutingStatusDto } from "../../shared/ipc/bindings";
import { errorText } from "../../shared/ipc/errors";
import { SettingsSection } from "./SettingsSection";
import {
  getSelectedDevice,
  getSelectedMicrophone,
  getSelectedMonitor,
  routingState,
  selectableDevices,
  setSelectedDevice,
  setSelectedMicrophone,
  setSelectedMonitor,
  type DiscoveryState,
  type SelectedDevice,
} from "./audioRoutingState";

const BLACKHOLE_URL = "https://existential.audio/blackhole/";
const INSTALLATION_URL = "https://github.com/ExistentialAudio/BlackHole/wiki/Installation";

export function AudioRouting() {
  const { t } = useTranslation();
  const [discovery, setDiscovery] = useState<DiscoveryState>({ status: "scanning" });
  const [selected, setSelected] = useState<SelectedDevice | null>(getSelectedDevice);
  const [microphone, setMicrophone] = useState<SelectedDevice | null>(getSelectedMicrophone);
  const [monitor, setMonitor] = useState<SelectedDevice | null>(getSelectedMonitor);
  const [route, setRoute] = useState<RoutingStatusDto | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [installerOpened, setInstallerOpened] = useState(false);
  const [linkFailed, setLinkFailed] = useState(false);
  const request = useRef(0);

  const refresh = useCallback(async () => {
    const current = ++request.current;
    setDiscovery({ status: "scanning" });
    try {
      const result = await commands.listAudioDevices();
      if (current !== request.current) return;
      setDiscovery(result.status === "ok" ? { status: "ready", result: result.data } : { status: "failed" });
    } catch {
      if (current === request.current) setDiscovery({ status: "failed" });
    }
  }, []);

  useEffect(() => {
    let mounted = true;
    void refresh();
    const checkRoute = async () => {
      try {
        const result = await commands.callRoutingStatus();
        if (mounted && result.status === "ok") setRoute(result.data);
      } catch { /* The onboarding state remains available outside Tauri. */ }
    };
    void checkRoute();
    const timer = window.setInterval(() => { if (document.visibilityState === "visible") void checkRoute(); }, 1000);
    return () => {
      mounted = false;
      window.clearInterval(timer);
      request.current++;
    };
  }, [refresh]);

  const state = routingState(discovery, selected);
  const stateText = {
    scanning: t("routing.state.scanning"),
    setupRequired: t("routing.state.setupRequired"),
    candidateDetected: t("routing.state.candidateDetected"),
    deviceSelected: t("routing.state.deviceSelected"),
    deviceUnavailable: t("routing.state.deviceUnavailable"),
    discoveryFailed: t("routing.state.discoveryFailed"),
    unsupportedConfiguration: t("routing.state.unsupportedConfiguration"),
  };
  const failureText = {
    noDevice: t("routing.failure.noDevice"),
    permissionDenied: t("routing.failure.permissionDenied"),
    unsupported: t("routing.failure.unsupported"),
    other: t("routing.failure.other"),
  };
  const devices = discovery.status === "ready" ? selectableDevices(discovery.result.devices) : [];
  const virtual = devices.filter((device) => device.kind === "virtual");
  const unknown = devices.filter((device) => device.kind === "unknown");
  const allDevices = discovery.status === "ready" ? discovery.result.devices : [];
  const microphones = allDevices.filter((device) => device.input && device.kind !== "virtual" && device.id !== selected?.id);
  const monitors = allDevices.filter((device) => device.output && device.kind !== "virtual" && device.id !== selected?.id);
  const running = route?.state === "running";
  const canStart = Boolean(selected && microphone && selected.id !== microphone.id &&
    allDevices.some((device) => device.id === selected.id && device.input && device.output && device.configuration === "supported") &&
    allDevices.some((device) => device.id === microphone.id && device.input && device.kind !== "virtual"));

  const start = async () => {
    if (!selected || !microphone || !canStart) return;
    setBusy(true); setActionError(null);
    try {
      const result = await commands.startCallRouting(microphone.id, selected.id);
      if (result.status === "ok") setRoute(result.data);
      else setActionError(errorText(t, result.error));
    } catch { setActionError(t("errors.backendUnreachable")); }
    setBusy(false);
  };

  const stop = async () => {
    setBusy(true); setActionError(null);
    try {
      const result = await commands.stopCallRouting();
      if (result.status === "ok") {
        const current = await commands.callRoutingStatus();
        if (current.status === "ok") setRoute(current.data);
      } else setActionError(errorText(t, result.error));
    } catch { setActionError(t("errors.backendUnreachable")); }
    setBusy(false);
  };

  const changeMonitor = async (id: string) => {
    const device = monitors.find((candidate) => candidate.id === id);
    setActionError(null);
    try {
      const result = await commands.setLocalMonitorOutput(device?.id ?? null);
      if (result.status === "error") { setActionError(errorText(t, result.error)); return; }
      const value = device ? { id: device.id, name: device.name } : null;
      setMonitor(value); setSelectedMonitor(value);
    } catch { setActionError(t("errors.backendUnreachable")); }
  };

  const openInstaller = async () => {
    setActionError(null); setInstallerOpened(false);
    try {
      const result = await commands.openAudioInstaller();
      if (result.status === "ok") setInstallerOpened(result.data);
      else setActionError(errorText(t, result.error));
    } catch { setActionError(t("errors.backendUnreachable")); }
  };

  const choose = (device: AudioDeviceDto) => {
    const value = { id: device.id, name: device.name };
    setSelectedDevice(value);
    setSelected(value);
  };

  const openPublisherUrl = async (url: typeof BLACKHOLE_URL | typeof INSTALLATION_URL) => {
    setLinkFailed(false);
    try {
      await openUrl(url);
    } catch {
      setLinkFailed(true);
    }
  };

  return (
    <SettingsSection title={t("routing.title")}>
      <div className="space-y-4 px-4 py-4 text-sm">
        <p className="text-muted-foreground">{t("routing.localPlayback")}</p>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p role="status" aria-live="polite" className="font-medium">
            {running ? t("routing.runRunning") : route?.state === "failed" ? t("routing.runError") : route ? `${t("routing.runStopped")} ${stateText[state]}` : stateText[state]}
          </p>
          <Button variant="outline" onClick={() => void refresh()}>{t("routing.refresh")}</Button>
        </div>
        {discovery.status === "ready" && discovery.result.incomplete && (
          <p role="status" className="text-muted-foreground">{t("routing.incomplete")}</p>
        )}
        {state === "deviceUnavailable" && selected && (
          <p className="text-destructive">{t("routing.unavailableName", { name: selected.name })}</p>
        )}
        {discovery.status === "ready" && devices.length > 0 && (
          <div className="space-y-4">
            {virtual.length > 0 && (
              <DeviceGroup title={t("routing.detectedOutputs")} devices={virtual} selected={selected} onChoose={choose} disabled={running} />
            )}
            {unknown.length > 0 && (
              <DeviceGroup title={t("routing.otherOutputs")} devices={unknown} selected={selected} onChoose={choose} disabled={running} />
            )}
          </div>
        )}
        {selected && state === "deviceSelected" && !running && (
          <p className="text-muted-foreground">{t("routing.selectionHint")}</p>
        )}
        <div className="space-y-3 border-t pt-4">
          <h3 className="font-medium">{t("routing.controlsTitle")}</h3>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1">
              <label htmlFor="route-microphone" className="font-medium">{t("routing.microphone")}</label>
              <ChoiceSelect id="route-microphone" disabled={running} value={microphone?.id ?? "__choose__"}
                options={[{ value: "__choose__", label: t("routing.chooseMicrophone") }, ...microphones.map((device) => ({ value: device.id, label: device.name }))]}
                onChange={(id) => { const device = microphones.find((candidate) => candidate.id === id); const value = device ? { id: device.id, name: device.name } : null; setMicrophone(value); setSelectedMicrophone(value); }} />
            </div>
            <div className="space-y-1">
              <label htmlFor="route-monitor" className="font-medium">{t("routing.monitor")}</label>
              <ChoiceSelect id="route-monitor" value={monitor?.id ?? "__default__"}
                options={[{ value: "__default__", label: t("routing.systemOutput") }, ...monitors.map((device) => ({ value: device.id, label: device.name }))]}
                onChange={(id) => { void changeMonitor(id); }} />
            </div>
          </div>
          <p className="text-muted-foreground">{t("routing.micPrivacy")}</p>
          <Button type="button" disabled={busy || (!running && !canStart)} onClick={() => void (running ? stop() : start())}>
            {running ? t("routing.stop") : t("routing.start")}
          </Button>
          {route?.failure && <p role="alert" className="text-destructive">{failureText[route.failure]}</p>}
          {running && (route.counters?.clipErrors ?? 0) > 0 && <p role="alert" className="text-destructive">{t("routing.clipFailed")}</p>}
          {actionError && <p role="alert" className="text-destructive">{actionError}</p>}
          {running && <p className="text-muted-foreground">{t("routing.chatInputHint", { name: selected?.name ?? "" })}</p>}
        </div>
        {state === "setupRequired" && (
          <div className="space-y-2 border-t pt-4">
            <h3 className="font-medium">{t("routing.setupTitle")}</h3>
            <p className="text-muted-foreground">{t("routing.setupHint")}</p>
            <Button type="button" variant="outline" onClick={() => void openInstaller()}>{t("routing.openInstaller")}</Button>
            {installerOpened && <p role="status">{t("routing.installerOpened")}</p>}
          </div>
        )}
        <div className="space-y-2 border-t pt-4">
          <h3 className="font-medium">{t("routing.manualTitle")}</h3>
          <ol className="list-inside list-decimal space-y-1 text-muted-foreground">
            <li>{t("routing.stepDownload")}</li>
            <li>{t("routing.stepInstall")}</li>
            <li>{t("routing.stepRefresh")}</li>
            <li>{t("routing.stepChat")}</li>
          </ol>
          <p>
            <a href={BLACKHOLE_URL} target="_blank" rel="noopener noreferrer" className="break-all text-primary underline underline-offset-4 focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring" onClick={(event) => { event.preventDefault(); void openPublisherUrl(BLACKHOLE_URL); }}>
              {t("routing.blackholeUrlLabel")}
            </a>
          </p>
          <p>
            <a href={INSTALLATION_URL} target="_blank" rel="noopener noreferrer" className="break-all text-primary underline underline-offset-4 focus-visible:rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring" onClick={(event) => { event.preventDefault(); void openPublisherUrl(INSTALLATION_URL); }}>
              {t("routing.instructionsUrlLabel")}
            </a>
          </p>
          {linkFailed && <p role="alert" className="text-destructive">{t("routing.linkFailed")}</p>}
        </div>
      </div>
    </SettingsSection>
  );
}

function DeviceGroup({
  title,
  devices,
  selected,
  onChoose,
  disabled,
}: {
  title: string;
  devices: AudioDeviceDto[];
  selected: SelectedDevice | null;
  onChoose: (device: AudioDeviceDto) => void;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div className="space-y-2">
      <h3 className="font-medium">{title}</h3>
      <ul className="space-y-2">
        {devices.map((device) => {
          const isSelected = selected?.id === device.id;
          return (
            <li key={device.id} className="flex flex-wrap items-center justify-between gap-2 rounded-lg border p-3">
              <div className="min-w-0">
                <p className="break-words font-medium">{device.name}</p>
                <p className="text-xs text-muted-foreground">
                  {t(device.kind === "unknown" ? "routing.unknownType" : "routing.detected")}
                  {" · "}{t(device.configuration === "supported" ? "routing.supported" : device.configuration === "unsupported" ? "routing.unsupported" : "routing.unconfirmed")}
                  {" · "}{t(device.input ? "routing.inputDetected" : "routing.inputNotDetected")}
                  {isSelected && <> · {t("routing.selected")}</>}
                </p>
              </div>
              <Button
                variant={isSelected ? "secondary" : "outline"}
                aria-label={t(isSelected ? "routing.selectedDevice" : "routing.selectDevice", { name: device.name })}
                aria-pressed={isSelected}
                disabled={disabled || !device.input || device.configuration === "unsupported"}
                onClick={() => onChoose(device)}
              >
                {t(isSelected ? "routing.selected" : "routing.select")}
              </Button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
