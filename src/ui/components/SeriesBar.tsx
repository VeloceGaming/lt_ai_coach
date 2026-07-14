// Full-mode Fearless series strip: game tabs plus locked-history summary.

import type { GameRecord } from "../types";
import { useT } from "../stores/useI18nStore";

export function SeriesBar({ currentGame, completedGames, seriesHistory, onGameClick, onFinishGame }: { currentGame: number; completedGames: GameRecord[]; seriesHistory: { blue: string[]; red: string[] }; onGameClick: (game: number) => void; onFinishGame: () => void }) {
  const t = useT();
  return <div className="series-bar"><span className="series-label">{t("series.label")}</span><div className="series-games">{[1,2,3,4,5].map((g) => { const done = completedGames.some((r) => r.gameNumber === g); return <button key={g} type="button" className={`series-game-btn${g === currentGame ? " active" : ""}${done && g !== currentGame ? " done" : ""}`} onClick={() => onGameClick(g)}>G{g}{done && g !== currentGame && <span className="series-dot" />}</button>; })}</div><div className="series-history-summary">{seriesHistory.blue.length + seriesHistory.red.length ? <><span className="blue-text">{seriesHistory.blue.length} {t("series.blueLocked")}</span><span className="sep">·</span><span className="red-text">{seriesHistory.red.length} {t("series.redLocked")}</span></> : <span className="muted">{t("series.noHistory")}</span>}</div>{currentGame < 5 && <button type="button" className="btn-sm series-finish-btn" onClick={onFinishGame}>{t("series.finish")} G{currentGame} →</button>}</div>;
}
