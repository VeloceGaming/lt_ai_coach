// Settings → Appearance + Coach Behavior.
// Appearance: theme mode, surface preset, typography, accent, reduce-motion,
// and the UI language (Stage 1 of translation support — see useI18nStore).
// Coach Behavior: draft strategy preset (Conservative / Balanced / Aggressive)
// with an optional Custom panel that exposes the raw tuning knobs.

import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DEFAULT_ACCENT, resolveMode, SURFACE_PREVIEW, type SurfacePreset, type ThemeMode, type TypographyStyle } from "../lib/theme";
import { en } from "../lib/i18n/strings";
import { useThemeStore } from "../stores/useThemeStore";
import { useI18nStore, usesSystemTypography, useT } from "../stores/useI18nStore";
import { usePreferencesStore } from "../stores/usePreferencesStore";
import { activeTuning, STRATEGY_TUNING } from "../lib/preferences";
import type { DraftStrategy, DraftTuning, ScoringWeights } from "../types";

const MODES: { id: ThemeMode; key: string }[] = [
  { id: "light", key: "settings.mode.light" },
  { id: "dark", key: "settings.mode.dark" },
  { id: "auto", key: "settings.mode.auto" },
];

const SURFACES: { id: SurfacePreset; key: string }[] = [
  { id: "neutral", key: "settings.surface.neutral" },
  { id: "warm", key: "settings.surface.warm" },
  { id: "broadcast", key: "settings.surface.broadcast" },
];

const TYPOGRAPHIES: { id: TypographyStyle; key: string; family: string }[] = [
  { id: "technical", key: "settings.typography.technical", family: '"JetBrains Mono", monospace' },
  { id: "clean", key: "settings.typography.clean", family: '"Inter", sans-serif' },
  { id: "geometric", key: "settings.typography.geometric", family: '"Space Grotesk", sans-serif' },
];

// Suggested accents; the first is the built-in default.
const ACCENTS = [DEFAULT_ACCENT, "#1d9e75", "#7c5cff", "#e2683c", "#c0427a", "#2bb3c0", "#d6a84e"];

const STRATEGIES: { id: DraftStrategy; key: string }[] = [
  { id: "conservative", key: "settings.strategy.conservative" },
  { id: "balanced", key: "settings.strategy.balanced" },
  { id: "aggressive", key: "settings.strategy.aggressive" },
  { id: "custom", key: "settings.strategy.custom" },
];

const TUNING_KNOBS: { key: keyof DraftTuning; textKey: string; min: number; max: number; step: number; format: (v: number) => string }[] = [
  { key: "patchMaxShift", textKey: "settings.tuning.patchMaxShift", min: 0.01, max: 0.25, step: 0.01, format: (v) => `${(v * 100).toFixed(0)}%` },
  { key: "patchImpactScale", textKey: "settings.tuning.patchImpactScale", min: 5, max: 100, step: 5, format: (v) => v.toFixed(0) },
  { key: "patchEvidenceGames", textKey: "settings.tuning.patchEvidenceGames", min: 5, max: 60, step: 5, format: (v) => `${v.toFixed(0)} games` },
  { key: "winRateRiskZ", textKey: "settings.tuning.winRateRiskZ", min: 0, max: 2, step: 0.05, format: (v) => v.toFixed(2) },
  { key: "winRatePriorGames", textKey: "settings.tuning.winRatePriorGames", min: 5, max: 60, step: 5, format: (v) => `${v.toFixed(0)} games` },
];

const SCORING_KEYS: Record<keyof ScoringWeights, string> = {
  performance: "settings.scoring.performance",
  synergy: "settings.scoring.synergy",
  matchup: "settings.scoring.matchup",
  flexibility: "settings.scoring.flexibility",
  draftOrder: "settings.scoring.draftOrder",
  draftPresence: "settings.scoring.draftPresence",
};

export function SettingsScreen() {
  const t = useT();
  const mode = useThemeStore((s) => s.mode);
  const surface = useThemeStore((s) => s.surface);
  const accent = useThemeStore((s) => s.accent);
  const reduceMotion = useThemeStore((s) => s.reduceMotion);
  const typography = useThemeStore((s) => s.typography);
  const setMode = useThemeStore((s) => s.setMode);
  const setSurface = useThemeStore((s) => s.setSurface);
  const setAccent = useThemeStore((s) => s.setAccent);
  const setReduceMotion = useThemeStore((s) => s.setReduceMotion);
  const setTypography = useThemeStore((s) => s.setTypography);

  const languageId = useI18nStore((s) => s.languageId);
  const languages = useI18nStore((s) => s.languages);
  const setLanguage = useI18nStore((s) => s.setLanguage);
  const refreshLanguages = useI18nStore((s) => s.refreshLanguages);
  const languageWarnings = useI18nStore((s) => s.languageWarnings);
  const [languageMessage, setLanguageMessage] = useState<string | null>(null);
  const typographyLocked = usesSystemTypography(languageId);

  // Pick up any translation file the user just dropped in without needing an
  // app restart.
  useEffect(() => { refreshLanguages(); }, [refreshLanguages]);

  function handleOpenTranslationsFolder() {
    invoke("open_translations_folder").catch((error) => setLanguageMessage(error instanceof Error ? error.message : String(error)));
  }

  function handleExportTemplate() {
    invoke<string>("export_translation_template", { entries: en })
      .then((path) => { setLanguageMessage(`${t("settings.language.exportSuccess")} ${path}`); refreshLanguages(); })
      .catch((error) => setLanguageMessage(error instanceof Error ? error.message : String(error)));
  }

  const strategy = usePreferencesStore((s) => s.strategy);
  const bansPerSide = usePreferencesStore((s) => s.bansPerSide);
  const customTuning = usePreferencesStore((s) => s.customTuning);
  const setStrategy = usePreferencesStore((s) => s.setStrategy);
  const setBansPerSide = usePreferencesStore((s) => s.setBansPerSide);
  const setCustomTuning = usePreferencesStore((s) => s.setCustomTuning);
  const weights = usePreferencesStore((s) => s.weights);
  const setWeight = usePreferencesStore((s) => s.setWeight);
  const minimumInteractionGames = usePreferencesStore((s) => s.minimumInteractionGames);
  const setMinimumInteractionGames = usePreferencesStore((s) => s.setMinimumInteractionGames);

  const effective = resolveMode(mode);
  const currentAccent = accent || DEFAULT_ACCENT;
  const activeTuningValues = activeTuning(strategy, customTuning);
  const activeStrategyKey = STRATEGIES.find((s) => s.id === strategy)?.key;

  return <div className="settings-screen">
    <section className="settings-section">
      <div className="settings-section-head"><span className="eyebrow">{t("settings.section.appearance")}</span><h2>{t("settings.section.theme")}</h2></div>

      <div className="settings-row">
        <div className="settings-label"><strong>{t("settings.mode.label")}</strong><span>{t("settings.mode.desc")}</span></div>
        <div className="settings-segmented" role="group" aria-label={t("settings.mode.label")}>
          {MODES.map((option) => <button type="button" key={option.id} className={mode === option.id ? "active" : ""} aria-pressed={mode === option.id} onClick={() => setMode(option.id)}>{t(option.key)}</button>)}
        </div>
      </div>

      <div className="settings-row column">
        <div className="settings-label"><strong>{t("settings.surface.label")}</strong><span>{t("settings.surface.desc")}</span></div>
        <div className="settings-presets">
          {SURFACES.map((option) => {
            const preview = SURFACE_PREVIEW[option.id][effective];
            return <button type="button" key={option.id} className={`settings-preset${surface === option.id ? " active" : ""}`} aria-pressed={surface === option.id} onClick={() => setSurface(option.id)}>
              <span className="settings-preset-swatch" style={{ background: preview.bg }}>
                <span className="settings-preset-card" style={{ background: preview.card, borderColor: preview.border }}>
                  <span className="settings-preset-dot" style={{ background: currentAccent }} />
                  <span className="settings-preset-line" style={{ background: preview.text }} />
                  <span className="settings-preset-line short" style={{ background: preview.text }} />
                </span>
              </span>
              <span className="settings-preset-name">{t(`${option.key}.label`)}</span>
              <span className="settings-preset-desc">{t(`${option.key}.desc`)}</span>
            </button>;
          })}
        </div>
      </div>

      <div className="settings-row column">
        <div className="settings-label"><strong>{t("settings.typography.label")}</strong><span>{t(typographyLocked ? "settings.typography.systemFontDesc" : "settings.typography.desc")}</span></div>
        <div className="settings-type-presets" role="group" aria-label={t("settings.typography.label")}>
          {TYPOGRAPHIES.map((option) => (
            <button
              type="button"
              key={option.id}
              className={`settings-type-preset${typography === option.id ? " active" : ""}`}
              aria-pressed={typography === option.id}
              disabled={typographyLocked}
              onClick={() => setTypography(option.id)}
              style={typographyLocked ? undefined : { fontFamily: option.family }}
            >
              <span className="settings-type-sample">Aa 92.4%</span>
              <span className="settings-preset-name">{t(`${option.key}.label`)}</span>
              <span className="settings-preset-desc">{t(`${option.key}.desc`)}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="settings-row">
        <div className="settings-label"><strong>{t("settings.accent.label")}</strong><span>{t("settings.accent.desc")}</span></div>
        <div className="settings-accent">
          <label className="settings-accent-picker" style={{ background: currentAccent }}>
            <input type="color" value={currentAccent} onChange={(event) => setAccent(event.target.value)} aria-label={t("settings.accent.customAria")} />
          </label>
          <div className="settings-accent-swatches">
            {ACCENTS.map((color) => <button type="button" key={color} className={`settings-swatch${currentAccent.toLowerCase() === color.toLowerCase() ? " active" : ""}`} style={{ background: color }} aria-label={color === DEFAULT_ACCENT ? t("settings.accent.defaultAria") : color} onClick={() => setAccent(color === DEFAULT_ACCENT ? "" : color)} />)}
          </div>
          <button type="button" className="settings-reset" onClick={() => setAccent("")}>{t("settings.accent.reset")}</button>
        </div>
      </div>

      <div className="settings-row">
        <div className="settings-label"><strong>{t("settings.reduceMotion.label")}</strong><span>{t("settings.reduceMotion.desc")}</span></div>
        <label className="settings-switch">
          <input type="checkbox" checked={reduceMotion} onChange={(event) => setReduceMotion(event.target.checked)} aria-label={t("settings.reduceMotion.label")} />
          <span className="settings-switch-track" aria-hidden="true" />
        </label>
      </div>

      <div className="settings-row column">
        <div className="settings-label"><strong>{t("settings.language.label")}</strong><span>{t("settings.language.desc")}</span></div>
        <div className="settings-language-row">
          <select className="settings-language-select" value={languageId} onChange={(event) => setLanguage(event.target.value)} aria-label={t("settings.language.label")}>
            <option value="en">{t("settings.language.en")}</option>
            {languages.map((lang) => <option key={lang.id} value={lang.id}>{lang.name}</option>)}
          </select>
          <button type="button" className="secondary-button" onClick={handleOpenTranslationsFolder}>{t("settings.language.openFolder")}</button>
          <button type="button" className="secondary-button" onClick={handleExportTemplate}>{t("settings.language.exportTemplate")}</button>
        </div>
        {languageMessage && <p className="settings-strategy-desc">{languageMessage}</p>}
        <p className="settings-strategy-desc">{t("settings.language.exportHint")}</p>
        {languageWarnings.map((warning) => <p className="settings-strategy-desc" key={warning}>{warning}</p>)}
      </div>
    </section>

    <section className="settings-section">
      <div className="settings-section-head"><span className="eyebrow">{t("settings.section.coach")}</span><h2>{t("settings.section.draftStrategy")}</h2></div>

      <div className="settings-row">
        <div className="settings-label"><strong>{t("settings.bansPerSide.label")}</strong><span>{t("settings.bansPerSide.desc")}</span></div>
        <div className="settings-segmented" role="group" aria-label={t("settings.bansPerSide.label")}>
          {[1, 2, 3, 4, 5].map((value) => <button type="button" key={value} className={bansPerSide === value ? "active" : ""} aria-pressed={bansPerSide === value} onClick={() => setBansPerSide(value)}>{value}</button>)}
        </div>
      </div>

      <div className="settings-row column">
        <div className="settings-label">
          <strong>{t("settings.strategy.label")}</strong>
          <span>{t("settings.strategy.desc")}</span>
        </div>
        <div className="settings-segmented strategy-segmented" role="group" aria-label={t("settings.strategy.label")}>
          {STRATEGIES.map((option) => (
            <button
              type="button"
              key={option.id}
              className={strategy === option.id ? "active" : ""}
              aria-pressed={strategy === option.id}
              onClick={() => {
                if (option.id === "custom") {
                  // Seed custom knobs from the current active preset so it's a
                  // useful starting point rather than jumping to stale values.
                  const base = strategy !== "custom" ? STRATEGY_TUNING[strategy] : customTuning;
                  (Object.keys(base) as (keyof DraftTuning)[]).forEach((k) => setCustomTuning(k, base[k]));
                } else {
                  setStrategy(option.id);
                }
              }}
            >
              {t(`${option.key}.label`)}
            </button>
          ))}
        </div>
        <p className="settings-strategy-desc">{activeStrategyKey ? t(`${activeStrategyKey}.desc`) : ""}</p>
      </div>

      {strategy === "custom" && (
        <div className="settings-custom-tuning">
          <div className="settings-section-head" style={{ paddingTop: 0 }}><span className="eyebrow">{t("settings.customTuning.eyebrow")}</span></div>
          {TUNING_KNOBS.map(({ key, textKey, min, max, step, format }) => (
            <div className="settings-row" key={key}>
              <div className="settings-label">
                <strong>{t(`${textKey}.label`)}</strong>
                <span>{t(`${textKey}.desc`)}</span>
              </div>
              <span className="range-control">
                <input
                  type="range"
                  min={min}
                  max={max}
                  step={step}
                  value={activeTuningValues[key]}
                  onChange={(e) => setCustomTuning(key, Number(e.target.value))}
                />
                <span className="range-endpoints" aria-hidden="true">
                  <small>{format(min)}</small>
                  <small>{format(max)}</small>
                </span>
                <output>{format(activeTuningValues[key])}</output>
              </span>
            </div>
          ))}
          <div className="settings-row">
            <div className="settings-label"><strong>{t("settings.resetPreset.label")}</strong><span>{t("settings.resetPreset.desc")}</span></div>
            <div className="settings-segmented" role="group" aria-label={t("settings.resetPreset.label")}>
              {(["conservative", "balanced", "aggressive"] as const).map((preset) => (
                <button
                  type="button"
                  key={preset}
                  onClick={() => {
                    const base = STRATEGY_TUNING[preset];
                    (Object.keys(base) as (keyof DraftTuning)[]).forEach((k) => setCustomTuning(k, base[k]));
                  }}
                >
                  {t(`settings.strategy.${preset}.label`)}
                </button>
              ))}
            </div>
          </div>
        </div>
      )}
    </section>

    <section className="settings-section scoring-section">
      <div className="settings-section-head"><span className="eyebrow">{t("settings.section.coach")}</span><h2>{t("settings.section.scoring")}</h2></div>
      <p className="settings-strategy-desc" style={{ marginBottom: 8 }}>
        {t("settings.scoring.desc")}
      </p>

      {(Object.keys(weights) as Array<keyof ScoringWeights>).map((key) => (
        <div className="settings-row" key={key}>
          <div className="settings-label"><strong>{t(`${SCORING_KEYS[key]}.label`)}</strong><span>{t(`${SCORING_KEYS[key]}.desc`)}</span></div>
          <span className="range-control">
            <input type="range" min="0" max="100" step="5" value={weights[key]} onChange={(e) => setWeight(key, Number(e.target.value))} />
            <span className="range-endpoints" aria-hidden="true"><small>0</small><small>100</small></span>
            <output>{weights[key]}</output>
          </span>
        </div>
      ))}

      <div className="settings-row">
        <div className="settings-label">
          <strong>{t("settings.minInteraction.label")}</strong>
          <span>{t("settings.minInteraction.desc")}</span>
        </div>
        <input
          type="number"
          min="1"
          max="50"
          value={minimumInteractionGames}
          onChange={(e) => setMinimumInteractionGames(Math.max(1, Math.min(50, Number(e.target.value) || 1)))}
          style={{ width: 64, textAlign: "center" }}
        />
      </div>
    </section>
  </div>;
}
