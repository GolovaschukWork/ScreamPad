import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/button";
import { commands, type AudioDeviceDto } from "../../shared/ipc/bindings";
import { SettingsSection } from "./SettingsSection";
import {
  getSelectedDevice,
  routingState,
  selectableDevices,
  setSelectedDevice,
  type DiscoveryState,
  type SelectedDevice,
} from "./audioRoutingState";

const BLACKHOLE_URL = "https://existential.audio/blackhole/";
const INSTALLATION_URL = "https://github.com/ExistentialAudio/BlackHole/wiki/Installation";

export function AudioRouting() {
  const { t } = useTranslation();
  const [discovery, setDiscovery] = useState<DiscoveryState>({ status: "scanning" });
  const [selected, setSelected] = useState<SelectedDevice | null>(getSelectedDevice);
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
    void refresh();
    return () => {
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
  const devices = discovery.status === "ready" ? selectableDevices(discovery.result.devices) : [];
  const virtual = devices.filter((device) => device.kind === "virtual");
  const unknown = devices.filter((device) => device.kind === "unknown");

  const choose = (device: AudioDeviceDto) => {
    const value = { id: device.id, name: device.name };
    setSelectedDevice(value);
    setSelected(value);
  };

  return (
    <SettingsSection title={t("routing.title")}>
      <div className="space-y-4 px-4 py-4 text-sm">
        <p className="text-muted-foreground">{t("routing.localPlayback")}</p>
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p role="status" aria-live="polite" className="font-medium">
            {stateText[state]}
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
              <DeviceGroup title={t("routing.detectedOutputs")} devices={virtual} selected={selected} onChoose={choose} />
            )}
            {unknown.length > 0 && (
              <DeviceGroup title={t("routing.otherOutputs")} devices={unknown} selected={selected} onChoose={choose} />
            )}
          </div>
        )}
        {selected && state === "deviceSelected" && (
          <p className="text-muted-foreground">{t("routing.selectionHint")}</p>
        )}
        <div className="space-y-2 border-t pt-4">
          <h3 className="font-medium">{t("routing.manualTitle")}</h3>
          <ol className="list-inside list-decimal space-y-1 text-muted-foreground">
            <li>{t("routing.stepDownload")}</li>
            <li>{t("routing.stepInstall")}</li>
            <li>{t("routing.stepRefresh")}</li>
            <li>{t("routing.stepChat")}</li>
          </ol>
          <p>{t("routing.blackholeUrlLabel")}: <span className="break-all select-text">{BLACKHOLE_URL}</span></p>
          <p>{t("routing.instructionsUrlLabel")}: <span className="break-all select-text">{INSTALLATION_URL}</span></p>
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
}: {
  title: string;
  devices: AudioDeviceDto[];
  selected: SelectedDevice | null;
  onChoose: (device: AudioDeviceDto) => void;
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
                  {isSelected && <> · {t("routing.selected")}</>}
                </p>
              </div>
              <Button
                variant={isSelected ? "secondary" : "outline"}
                aria-label={t(isSelected ? "routing.selectedDevice" : "routing.selectDevice", { name: device.name })}
                aria-pressed={isSelected}
                disabled={device.configuration === "unsupported"}
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
