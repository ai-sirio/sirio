import test from "node:test";
import assert from "node:assert/strict";
import {
  assertLayout,
  createInitialState,
  moveTab,
  reorderTab,
  resizeSplit,
  splitWithTab
} from "./layout-model.mjs";

const tabIDs = (state, groupID) => state.groups[groupID].tabs.map((tab) => tab.id);

function leaves(node) {
  if (node.type === "group") return [node.groupID];
  return [...leaves(node.first), ...leaves(node.second)];
}

test("reorders a tab inside its pane group", () => {
  const state = reorderTab(createInitialState(), {
    groupID: "A",
    tabID: "terminal-1",
    toIndex: 2
  });

  assert.deepEqual(tabIDs(state, "A"), ["claude", "terminal-1"]);
  assert.equal(state.groups.A.activeTabID, "terminal-1");
  assert.equal(state.activeGroupID, "A");
  assertLayout(state);
});

test("moves a tab atomically between pane groups", () => {
  const state = moveTab(createInitialState(), {
    fromGroupID: "A",
    toGroupID: "B",
    tabID: "claude",
    toIndex: 0
  });

  assert.deepEqual(tabIDs(state, "A"), ["terminal-1"]);
  assert.deepEqual(tabIDs(state, "B"), ["claude", "appmodel"]);
  assert.equal(state.groups.B.activeTabID, "claude");
  assert.equal(state.activeGroupID, "B");
  assertLayout(state);
});

test("collapses a non-root source group when its last tab moves", () => {
  const state = moveTab(createInitialState(), {
    fromGroupID: "B",
    toGroupID: "C",
    tabID: "appmodel",
    toIndex: 1
  });

  assert.equal(state.groups.B, undefined);
  assert.deepEqual(leaves(state.root), ["A", "C"]);
  assert.deepEqual(tabIDs(state, "C"), ["readme", "appmodel"]);
  assert.equal(state.lastMutation.kind, "move-and-collapse");
  assertLayout(state);
});

test("creates a right split while preserving the target group identity", () => {
  const state = splitWithTab(createInitialState(), {
    fromGroupID: "A",
    targetGroupID: "B",
    tabID: "claude",
    edge: "right"
  });

  const newGroupID = state.activeGroupID;
  assert.notEqual(newGroupID, "A");
  assert.notEqual(newGroupID, "B");
  assert.deepEqual(tabIDs(state, newGroupID), ["claude"]);
  assert.deepEqual(tabIDs(state, "A"), ["terminal-1"]);
  assert.deepEqual(leaves(state.root), ["A", "B", newGroupID, "C"]);

  const targetSplit = state.root.second.first;
  assert.equal(targetSplit.type, "split");
  assert.equal(targetSplit.axis, "horizontal");
  assert.equal(targetSplit.first.groupID, "B");
  assert.equal(targetSplit.second.groupID, newGroupID);
  assertLayout(state);
});

test("splits one tab out of its existing multi-tab group", () => {
  const state = splitWithTab(createInitialState(), {
    fromGroupID: "A",
    targetGroupID: "A",
    tabID: "claude",
    edge: "left"
  });

  const newGroupID = state.activeGroupID;
  assert.deepEqual(tabIDs(state, "A"), ["terminal-1"]);
  assert.deepEqual(tabIDs(state, newGroupID), ["claude"]);
  assert.equal(state.root.first.type, "split");
  assert.equal(state.root.first.axis, "horizontal");
  assert.equal(state.root.first.first.groupID, newGroupID);
  assert.equal(state.root.first.second.groupID, "A");
  assertLayout(state);
});

test("rejects splitting the sole tab out of its own group", () => {
  assert.throws(
    () => splitWithTab(createInitialState(), {
      fromGroupID: "B",
      targetGroupID: "B",
      tabID: "appmodel",
      edge: "bottom"
    }),
    /sole tab/
  );
});

test("resizes by stable split identity and clamps the preferred fraction", () => {
  const state = resizeSplit(createInitialState(), {
    splitID: "split-root",
    fraction: 0.94
  });

  assert.equal(state.root.id, "split-root");
  assert.equal(state.root.fraction, 0.82);
  assert.equal(state.lastMutation.kind, "resize");
  assertLayout(state);
});
