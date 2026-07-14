// Projected role assignments for one team, shown beneath its picks.

import type { TeamProjection } from "../types";
import { formatPercent } from "../lib/format";

export function LineupProjection({ projection }: { projection: TeamProjection | null }) {
  if (!projection?.champions.length) return null;
  return <div className="lineup-projection"><div className="projection-heading"><span>Projected roles</span><small>{formatPercent(projection.confidence)} conf · {projection.assignmentsConsidered} assigns</small></div>{projection.champions.map((champion) => { const hasAssignedRole = champion.roles.some((role) => role.assigned); return <div className="projection-row" key={champion.championId}><strong>{champion.championName}</strong><div>{!hasAssignedRole && <span className="undetermined-role">Role undetermined</span>}{champion.roles.slice(0, 3).map((role) => <span className={role.assigned ? "assigned-role" : undefined} key={role.role}>{role.role} {Math.round(role.probability * 100)}%</span>)}</div></div>; })}</div>;
}
