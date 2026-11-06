import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SystemInfo } from "@/shared/types";
import type { Updater } from "@/client/lib/updater";
import { Button, Modal } from "./ui";
import { t } from "@/client/i18n";

const REPO = "https://github.com/xxxronelxxx/diskmaster";

function Row({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-start justify-between gap-4 border-b border-white/[0.06] py-1.5 last:border-0">
      <span className="text-[12px] text-slate-500">{label}</span>
      <span className={`text-right text-[12px] text-slate-200 ${mono ? "font-mono text-[11px]" : ""}`}>
        {value}
      </span>
    </div>
  );
}

export function AboutModal({
  open,
  onClose,
  info,
  updater,
}: {
  open: boolean;
  onClose: () => void;
  info?: SystemInfo;
  updater: Updater;
}) {
  const [copied, setCopied] = useState(false);

  async function copyDiagnostics() {
    try {
      await invoke("copy_diagnostics");
      setCopied(true);
      setTimeout(() => setCopied(false), 2500);
    } catch {
      setCopied(false);
    }
  }

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t("about")}
      description="Диск-Мастер — очистка диска, анализ места и мониторинг Windows."
      footer={<Button variant="secondary" onClick={onClose}>{t("close")}</Button>}
    >
      <div className="space-y-0.5">
        <Row label="Версия" value={`v${updater.version || "…"}`} />
        <Row label="Режим" value={updater.portable ? "портативная" : "установленная"} />
        <Row label="Издатель" value="Development SPB" />
        {info ? <Row label="Система" value={`${info.os} ${info.version} (${info.build})`} /> : null}
        {info ? <Row label="Права" value={info.admin ? "администратор" : "обычные"} /> : null}
        <Row label="Данные" value="%LOCALAPPDATA%\DiskMasterData" mono />
        <Row label="Журнал" value="…\DiskMasterData\logs\app.log" mono />
      </div>

      <div className="mt-4 flex flex-wrap gap-2">
        <Button size="sm" variant="secondary" onClick={() => void invoke("open_url", { url: REPO }).catch(() => undefined)}>
          GitHub
        </Button>
        <Button size="sm" variant="secondary" onClick={() => void invoke("open_releases").catch(() => undefined)}>
          Релизы
        </Button>
        <Button size="sm" variant="secondary" onClick={() => void invoke("open_logs").catch(() => undefined)}>
          Открыть журнал
        </Button>
        <Button size="sm" variant="secondary" onClick={() => void copyDiagnostics()}>
          {copied ? "Скопировано" : t("copyDiagnostics")}
        </Button>
        <Button size="sm" variant="primary" onClick={() => void updater.checkNow()}>
          {t("checkUpdates")}
        </Button>
      </div>

      {updater.notice ? (
        <p className="mt-3 text-[11px] text-slate-400">{updater.notice}</p>
      ) : updater.update ? (
        <p className="mt-3 text-[11px] text-emerald-300">
          Доступна версия {updater.update.version}
        </p>
      ) : null}
    </Modal>
  );
}
