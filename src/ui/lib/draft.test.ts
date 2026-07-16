// Shadow-pick layer: merging hypothetical champions over the real draft, and
// the store rules that keep the real board authoritative.

import { beforeEach, describe, expect, it } from "vitest";
import { emptyDraft, emptyShadow, hasShadows, mergeShadow } from "./draft";
import { useDraftStore } from "../stores/useDraftStore";

const draft = (over: Partial<typeof emptyDraft>) => ({ ...structuredClone(emptyDraft), ...over });
const shadow = (over: Partial<typeof emptyShadow>) => ({ ...emptyShadow, ...over });

describe("mergeShadow", () => {
  it("appends shadows onto the open slots after the real entries", () => {
    const merged = mergeShadow(
      draft({ bluePicks: ["real_one"] }),
      shadow({ bluePicks: ["ghost_one", "ghost_two"] }),
    );
    expect(merged.bluePicks).toEqual(["real_one", "ghost_one", "ghost_two"]);
  });

  it("drops a shadow that meanwhile landed anywhere on the real board", () => {
    const merged = mergeShadow(
      draft({ redBans: ["contested"] }),
      shadow({ bluePicks: ["contested", "ghost"] }),
    );
    expect(merged.bluePicks).toEqual(["ghost"]);
  });

  it("drops shadows past the slot caps", () => {
    const merged = mergeShadow(
      draft({ bluePicks: ["a", "b", "c", "d"] }),
      shadow({ bluePicks: ["e", "f"] }),
      3,
    );
    expect(merged.bluePicks).toEqual(["a", "b", "c", "d", "e"]);
    const bans = mergeShadow(draft({ blueBans: ["x"] }), shadow({ blueBans: ["y", "z"] }), 2);
    expect(bans.blueBans).toEqual(["x", "y"]);
  });

  it("hasShadows reports whether any list holds an entry", () => {
    expect(hasShadows(emptyShadow)).toBe(false);
    expect(hasShadows(shadow({ redPicks: ["ghost"] }))).toBe(true);
  });
});

describe("useDraftStore shadow layer", () => {
  beforeEach(() => {
    useDraftStore.setState({ draft: structuredClone(emptyDraft), history: [], shadow: emptyShadow, roleOverrides: {} });
  });

  it("stages a shadow without touching the real draft", () => {
    const ok = useDraftStore.getState().pushShadowChampion("ghost", "blue-pick", 3, "normal");
    expect(ok).toBe(true);
    expect(useDraftStore.getState().shadow.bluePicks).toEqual(["ghost"]);
    expect(useDraftStore.getState().draft.bluePicks).toEqual([]);
  });

  it("rejects a shadow already on the imagined board and history actions", () => {
    useDraftStore.setState({ draft: draft({ redPicks: ["taken"] }) });
    const store = useDraftStore.getState();
    expect(store.pushShadowChampion("taken", "blue-pick", 3, "normal")).toBe(false);
    expect(store.pushShadowChampion("ghost", "blue-pick", 3, "normal")).toBe(true);
    expect(useDraftStore.getState().pushShadowChampion("ghost", "blue-pick", 3, "normal")).toBe(false);
    expect(useDraftStore.getState().pushShadowChampion("any", "history-blue", 3, "normal")).toBe(false);
  });

  it("bridge updates evict shadows the real draft collided with", () => {
    useDraftStore.setState({ shadow: shadow({ bluePicks: ["ghost", "sniped"], redBans: ["ghost_ban"] }), shadowEvictions: null });
    useDraftStore.getState().applyBridgeUpdate({
      blueBans: [],
      redBans: [],
      bluePicks: [],
      redPicks: ["sniped"],
    });
    const state = useDraftStore.getState();
    expect(state.shadow.bluePicks).toEqual(["ghost"]);
    expect(state.shadow.redBans).toEqual(["ghost_ban"]);
    expect(state.draft.redPicks).toEqual(["sniped"]);
    // The eviction is recorded with the ghost's old list, so the UI can play
    // the dissolve there and flash the replacing real slot; an update without
    // collisions leaves the record untouched.
    expect(state.shadowEvictions?.entries).toEqual([{ championId: "sniped", target: "bluePicks" }]);
    useDraftStore.getState().applyBridgeUpdate({ blueBans: [], redBans: [], bluePicks: [], redPicks: ["sniped"] });
    expect(useDraftStore.getState().shadowEvictions?.entries).toEqual([{ championId: "sniped", target: "bluePicks" }]);
  });

  it("a new real entry overwrites the shadow holding its slot instead of pushing it down", () => {
    useDraftStore.setState({ draft: draft({ bluePicks: ["existing"] }), shadow: shadow({ bluePicks: ["ghost_one", "ghost_two"] }), shadowEvictions: null });
    useDraftStore.getState().applyBridgeUpdate({
      blueBans: [],
      redBans: [],
      bluePicks: ["existing", "real_new"],
      redPicks: [],
    });
    const state = useDraftStore.getState();
    // ghost_one was consumed by real_new taking its slot; ghost_two survives.
    expect(state.shadow.bluePicks).toEqual(["ghost_two"]);
    // The eviction records the real taker so its slot plays the flash.
    expect(state.shadowEvictions?.entries).toEqual([{ championId: "real_new", target: "bluePicks" }]);
  });

  it("a shadow that became real in its own slot is not double-consumed", () => {
    useDraftStore.setState({ draft: draft({}), shadow: shadow({ bluePicks: ["ghost_one", "ghost_two"] }), shadowEvictions: null });
    useDraftStore.getState().applyBridgeUpdate({
      blueBans: [],
      redBans: [],
      bluePicks: ["ghost_one"],
      redPicks: [],
    });
    const state = useDraftStore.getState();
    // ghost_one solidified (collision); ghost_two must NOT also be consumed.
    expect(state.shadow.bluePicks).toEqual(["ghost_two"]);
    expect(state.shadowEvictions?.entries).toEqual([{ championId: "ghost_one", target: "bluePicks" }]);
  });

  it("clearShadows empties every list", () => {
    useDraftStore.setState({ shadow: shadow({ bluePicks: ["a"], blueBans: ["b"] }) });
    useDraftStore.getState().clearShadows();
    expect(hasShadows(useDraftStore.getState().shadow)).toBe(false);
  });
});
