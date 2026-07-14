LT AI COACH — FRONTEND OVERHAUL, STAGE B MOCKUPS
=================================================

What this folder is
-------------------
These are the locked "how it should look" mockups from Stage B of the frontend
overhaul, saved so a future work session (or you) can open them in a browser and
see EXACTLY what was agreed, instead of rebuilding from a written description.

They are static HTML pictures, not the real app. They are self-contained: open
any file by double-clicking it; it renders in any browser (it pulls the icon font
from a CDN, so keep an internet connection the first time).

The files
---------
compact-overlay.html
    The live-draft overlay (the wide, short, always-on-top strip).
    THIS ONE IS PIXEL-LOCKED — we iterated on it the most.
    Layout = three EQUAL thirds: board (left) | detail (middle) | picks (right).
    Board portraits are 42px wide. Click a pick card to update the middle detail.
    No "Pick" buttons anywhere (the app only reads the game, it cannot pick for you).

draft-board-flipcards.html
    The full-mode draft board, DEFAULT coach view -- this is the primary draft
    screen. The recommendations are vertical FLIP cards: champion portrait on the
    front, 3-4 reasons on the back. Click a card to flip it (it stays flipped until
    clicked again). No "Pick" buttons. Mode toggle + New draft + side swap in the
    chrome.

draft-board.html
    The same draft board in a Fearless example -- shows the Fearless-specific extras
    on top of the default board: the Normal/Fearless/Hard mode toggle, "New series"
    reset, the browse grid with side-coloured locks (blue lock = locked for blue
    only; red = locked for red; in Hard everything is locked for all), and the
    bottom series strip (G1-G5 + locked faces). CONCEPT mockup.

tier-list.html
    The full-mode tier list in the NEUTRAL (default) theme.
    STRUCTURE is locked: a dense portrait grid in coloured tier rows, buff/nerf
    badges + lane glyphs on each tile, and a right SIDE PANEL with the projected
    win rate, the interactive win-rate LINE CHART (previous -> current -> projected
    patch -- hover the points), the changed-this-patch list, the manual tier
    override (S-F), and a Patch Notes link.

All four mockups use the neutral default colour theme and adapt to your system's
light/dark setting. Colour is meant to be a user-customizable theme in the real
app; "neutral" is just the default that ships.

Things the placeholders do NOT show (handled in the real build)
---------------------------------------------------------------
- Champion art: the squares/figures are placeholders. Real champions are the
  game's tall, narrow full-body sprites, cropped from the sprite sheet.
- Lane glyphs: the real Top/Jungle/Mid/Bot/Support SVGs live in assets/Glyphs/
  and are recoloured to follow whichever theme is active.
- Colour theme: these files hardcode a colour set so they render standalone. In
  the app, colour comes from the design tokens (tokens.css) and is themeable.

The full written spec of every locked decision is in the project memory file
"stageb-locked-visual-language.md".
