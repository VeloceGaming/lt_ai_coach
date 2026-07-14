const roleFiles: Record<string, string> = {
  top: "/Glyphs/Top.svg",
  jungle: "/Glyphs/Jungle.svg",
  mid: "/Glyphs/Mid.svg",
  bot: "/Glyphs/Bot.svg",
  support: "/Glyphs/Support.svg",
};

export function RoleGlyph({ role, label }: { role: string; label?: string }) {
  const path = roleFiles[role.toLowerCase()];
  if (!path) return null;
  return <span className="role-glyph" role={label ? "img" : undefined} aria-label={label} aria-hidden={label ? undefined : true} style={{ maskImage: `url("${path}")`, WebkitMaskImage: `url("${path}")` }} />;
}
