import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { getStats, type StatsPayload } from "@/services/tauriApi";
import { SettingsSection } from "@/components/ui/SettingsSection";

type Loaded = { today: StatsPayload; week: StatsPayload; all: StatsPayload };

function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 1) return "0s";
  const s = Math.round(seconds);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rs = s % 60;
  if (m < 60) return rs > 0 ? `${m}m ${rs}s` : `${m}m`;
  const h = Math.floor(m / 60);
  const rm = m % 60;
  return rm > 0 ? `${h}h ${rm}m` : `${h}h`;
}

function formatCount(n: number): string {
  return Math.round(n).toLocaleString();
}

export default function StatisticsSection() {
  const { t } = useTranslation();
  const [data, setData] = useState<Loaded | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [today, week, all] = await Promise.all([
          getStats("today"),
          getStats("week"),
          getStats("all"),
        ]);
        if (!cancelled) setData({ today, week, all });
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (error) {
    return (
      <SettingsSection title={t("stats.title")} description={t("stats.description")}>
        <p className="text-sm text-destructive">{error}</p>
      </SettingsSection>
    );
  }

  if (!data) {
    // Loading skeleton — same shape as the loaded layout.
    return (
      <SettingsSection title={t("stats.title")} description={t("stats.description")}>
        <div className="grid grid-cols-2 gap-3">
          <div className="h-20 rounded-control bg-surface-1 animate-pulse" />
          <div className="h-20 rounded-control bg-surface-1 animate-pulse" />
        </div>
        <div className="h-4 mt-4 w-32 rounded bg-surface-1 animate-pulse" />
        <div className="space-y-2 mt-2">
          <div className="h-4 w-2/3 rounded bg-surface-1 animate-pulse" />
          <div className="h-4 w-2/3 rounded bg-surface-1 animate-pulse" />
          <div className="h-4 w-2/3 rounded bg-surface-1 animate-pulse" />
        </div>
      </SettingsSection>
    );
  }

  if (data.all.total_recordings === 0) {
    return (
      <SettingsSection title={t("stats.title")} description={t("stats.description")}>
        <p className="text-sm text-muted-foreground">{t("stats.empty")}</p>
      </SettingsSection>
    );
  }

  const { today, week, all } = data;

  return (
    <SettingsSection title={t("stats.title")} description={t("stats.description")}>
      {/* Big totals */}
      <div className="grid grid-cols-2 gap-3">
        <div className="rounded-control bg-surface-1 px-4 py-3">
          <p className="text-2xl font-semibold text-foreground-bright tabular-nums">
            {formatDuration(all.total_seconds)}
          </p>
          <p className="text-xs text-muted-foreground mt-0.5">{t("stats.totalAudio")}</p>
        </div>
        <div className="rounded-control bg-surface-1 px-4 py-3">
          <p className="text-2xl font-semibold text-foreground-bright tabular-nums">
            {formatCount(all.total_words)}
          </p>
          <p className="text-xs text-muted-foreground mt-0.5">{t("stats.totalWords")}</p>
        </div>
      </div>

      {/* Breakdown */}
      <div className="pt-2">
        <p className="text-xs uppercase tracking-wider text-muted-foreground/80 mb-2">
          {t("stats.breakdown")}
        </p>
        <ul className="space-y-1.5 text-sm">
          <BreakdownRow label={t("stats.today")} stats={today} />
          <BreakdownRow label={t("stats.thisWeek")} stats={week} />
          <BreakdownRow label={t("stats.allTime")} stats={all} />
        </ul>
      </div>

      {/* Average */}
      <p className="pt-3 text-xs text-muted-foreground">
        {t("stats.average", {
          duration: formatDuration(all.avg_seconds),
          words: formatCount(all.avg_words),
        })}
      </p>
    </SettingsSection>
  );
}

function BreakdownRow({ label, stats }: { label: string; stats: StatsPayload }) {
  const { t } = useTranslation();
  return (
    <li className="flex items-baseline justify-between gap-4">
      <span className="text-foreground">{label}</span>
      <span className="text-muted-foreground tabular-nums">
        {t("stats.recordings", { count: stats.total_recordings })} ·{" "}
        {formatDuration(stats.total_seconds)}
      </span>
    </li>
  );
}
