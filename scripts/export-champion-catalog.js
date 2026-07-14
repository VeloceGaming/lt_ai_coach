const fs = require("fs");
const path = require("path");

const sourcePath = process.argv[2];
const outputPath = process.argv[3];

if (!sourcePath || !outputPath) {
  throw new Error(
    "usage: node export-champion-catalog.js <meta-data.js> <champions.json>",
  );
}

const source = fs.readFileSync(sourcePath, "utf8");
const payload = source
  .replace(/^window\.TFM2_META_DATA=/, "")
  .replace(/;\s*$/, "");
const data = JSON.parse(payload);

const champions = (data.champions || []).map((champion) => ({
  id: champion.id,
  name: champion.name,
  category: champion.category,
  tags: champion.tags || [],
  rawTags: champion.rawTags || [],
  description: champion.description || {},
  stats: champion.stats || {},
  growth: champion.growth || {},
  skills: champion.skills || [],
  metrics: champion.metrics || {},
  roleFit: champion.roleFit || {},
  bestRole: champion.bestRole || null,
  asset: champion.asset || null,
  customChampion: Boolean(champion.customChampion),
}));

champions.sort((left, right) => left.id.localeCompare(right.id));
fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(
  outputPath,
  `${JSON.stringify(
    {
      schemaVersion: 1,
      generatedFrom: "TFM2 Meta Dashboard reduced champion catalog",
      generatedAt: data.generatedAt || null,
      champions,
    },
    null,
    2,
  )}\n`,
  "utf8",
);

console.log(`Exported ${champions.length} champions to ${outputPath}`);

