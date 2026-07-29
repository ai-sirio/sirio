const MIN_FRACTION = 0.18;
const MAX_FRACTION = 0.82;

export function createInitialState() {
  return {
    root: {
      type: "split",
      id: "split-root",
      axis: "horizontal",
      fraction: 0.58,
      first: { type: "group", groupID: "A" },
      second: {
        type: "split",
        id: "split-bc",
        axis: "vertical",
        fraction: 0.56,
        first: { type: "group", groupID: "B" },
        second: { type: "group", groupID: "C" }
      }
    },
    groups: {
      A: {
        id: "A",
        tabs: [
          { id: "terminal-1", title: "Terminal 1", kind: "terminal" },
          { id: "claude", title: "Claude", kind: "chat" }
        ],
        activeTabID: "terminal-1"
      },
      B: {
        id: "B",
        tabs: [{ id: "appmodel", title: "AppModel.swift", kind: "code" }],
        activeTabID: "appmodel"
      },
      C: {
        id: "C",
        tabs: [{ id: "readme", title: "README.md", kind: "markdown" }],
        activeTabID: "readme"
      }
    },
    activeGroupID: "A",
    nextGroupOrdinal: 1,
    nextSplitOrdinal: 1,
    lastMutation: { kind: "reset", label: "Initial three-group layout" }
  };
}

export function reorderTab(state, { groupID, tabID, toIndex }) {
  const next = structuredClone(state);
  const group = requireGroup(next, groupID);
  const fromIndex = group.tabs.findIndex((tab) => tab.id === tabID);
  if (fromIndex < 0) throw new Error(`Unknown tab ${tabID} in group ${groupID}`);

  const [tab] = group.tabs.splice(fromIndex, 1);
  const insertionIndex = clampIndex(toIndex, group.tabs.length);
  group.tabs.splice(insertionIndex, 0, tab);
  group.activeTabID = tabID;
  next.activeGroupID = groupID;
  next.lastMutation = {
    kind: "reorder",
    label: `Reordered ${tab.title} inside group ${groupID}`
  };
  assertLayout(next);
  return next;
}

export function moveTab(state, { fromGroupID, toGroupID, tabID, toIndex }) {
  if (fromGroupID === toGroupID) {
    return reorderTab(state, { groupID: fromGroupID, tabID, toIndex });
  }

  const next = structuredClone(state);
  const source = requireGroup(next, fromGroupID);
  const target = requireGroup(next, toGroupID);
  const tab = extractTab(source, tabID);
  const collapsed = collapseIfEmpty(next, fromGroupID);
  const insertionIndex = clampIndex(toIndex, target.tabs.length);
  target.tabs.splice(insertionIndex, 0, tab);
  target.activeTabID = tabID;
  next.activeGroupID = toGroupID;
  next.lastMutation = {
    kind: collapsed ? "move-and-collapse" : "move",
    label: collapsed
      ? `Moved ${tab.title} to group ${toGroupID}; collapsed empty group ${fromGroupID}`
      : `Moved ${tab.title} from group ${fromGroupID} to ${toGroupID}`
  };
  assertLayout(next);
  return next;
}

export function splitWithTab(state, { fromGroupID, targetGroupID, tabID, edge }) {
  if (!["top", "right", "bottom", "left"].includes(edge)) {
    throw new Error(`Unsupported split edge ${edge}`);
  }

  const currentSource = requireGroup(state, fromGroupID);
  requireGroup(state, targetGroupID);
  if (fromGroupID === targetGroupID && currentSource.tabs.length === 1) {
    throw new Error("Cannot split the sole tab out of its own group");
  }

  const next = structuredClone(state);
  const source = requireGroup(next, fromGroupID);
  const tab = extractTab(source, tabID);
  collapseIfEmpty(next, fromGroupID);

  const newGroupID = `G${next.nextGroupOrdinal++}`;
  const newSplitID = `split-${next.nextSplitOrdinal++}`;
  next.groups[newGroupID] = {
    id: newGroupID,
    tabs: [tab],
    activeTabID: tab.id
  };

  const targetLeaf = { type: "group", groupID: targetGroupID };
  const newLeaf = { type: "group", groupID: newGroupID };
  const newFirst = edge === "left" || edge === "top" ? newLeaf : targetLeaf;
  const newSecond = edge === "left" || edge === "top" ? targetLeaf : newLeaf;
  const replacement = {
    type: "split",
    id: newSplitID,
    axis: edge === "left" || edge === "right" ? "horizontal" : "vertical",
    fraction: 0.5,
    first: newFirst,
    second: newSecond
  };

  const replaced = replaceGroupNode(next.root, targetGroupID, replacement);
  if (!replaced.found) throw new Error(`Target group ${targetGroupID} is not in the layout`);
  next.root = replaced.node;
  next.activeGroupID = newGroupID;
  next.lastMutation = {
    kind: "split",
    label: `Split ${tab.title} ${edge} of group ${targetGroupID}`
  };
  assertLayout(next);
  return next;
}

export function resizeSplit(state, { splitID, fraction }) {
  const next = structuredClone(state);
  const resized = updateSplitFraction(next.root, splitID, clampFraction(fraction));
  if (!resized.found) throw new Error(`Unknown split ${splitID}`);
  next.root = resized.node;
  next.lastMutation = {
    kind: "resize",
    label: `Resized ${splitID} to ${Math.round(resized.fraction * 100)}%`
  };
  assertLayout(next);
  return next;
}

export function activateTab(state, { groupID, tabID }) {
  const next = structuredClone(state);
  const group = requireGroup(next, groupID);
  if (!group.tabs.some((tab) => tab.id === tabID)) {
    throw new Error(`Unknown tab ${tabID} in group ${groupID}`);
  }
  group.activeTabID = tabID;
  next.activeGroupID = groupID;
  next.lastMutation = { kind: "activate", label: `Activated ${tabID} in group ${groupID}` };
  assertLayout(next);
  return next;
}

export function assertLayout(state) {
  if (!state?.root || !state?.groups) throw new Error("Layout requires a root and groups");

  const leafIDs = [];
  const splitIDs = new Set();
  walk(state.root, (node) => {
    if (node.type === "group") {
      leafIDs.push(node.groupID);
      return;
    }
    if (splitIDs.has(node.id)) throw new Error(`Duplicate split identity ${node.id}`);
    splitIDs.add(node.id);
    if (!Number.isFinite(node.fraction) || node.fraction <= 0 || node.fraction >= 1) {
      throw new Error(`Invalid fraction for ${node.id}`);
    }
  });

  const uniqueLeaves = new Set(leafIDs);
  if (uniqueLeaves.size !== leafIDs.length) throw new Error("A pane group appears in more than one leaf");
  const registryIDs = Object.keys(state.groups);
  if (registryIDs.length !== uniqueLeaves.size || registryIDs.some((id) => !uniqueLeaves.has(id))) {
    throw new Error("Layout leaves and pane-group registry differ");
  }
  if (!state.groups[state.activeGroupID]) throw new Error("Active group is not in the registry");

  const seenTabs = new Set();
  for (const group of Object.values(state.groups)) {
    if (group.tabs.length === 0 && !(registryIDs.length === 1 && state.root.type === "group")) {
      throw new Error(`Non-root group ${group.id} is empty`);
    }
    if (group.tabs.length === 0 && group.activeTabID !== null) {
      throw new Error(`Empty group ${group.id} has an active tab`);
    }
    if (group.tabs.length > 0 && !group.tabs.some((tab) => tab.id === group.activeTabID)) {
      throw new Error(`Active tab for group ${group.id} is missing`);
    }
    for (const tab of group.tabs) {
      if (seenTabs.has(tab.id)) throw new Error(`Tab ${tab.id} belongs to more than one group`);
      seenTabs.add(tab.id);
    }
  }
  return true;
}

export function describeTopology(node) {
  if (node.type === "group") return node.groupID;
  const axis = node.axis === "horizontal" ? "H" : "V";
  return `${axis}${Math.round(node.fraction * 100)}(${describeTopology(node.first)}, ${describeTopology(node.second)})`;
}

function requireGroup(state, groupID) {
  const group = state.groups[groupID];
  if (!group) throw new Error(`Unknown group ${groupID}`);
  return group;
}

function extractTab(group, tabID) {
  const index = group.tabs.findIndex((tab) => tab.id === tabID);
  if (index < 0) throw new Error(`Unknown tab ${tabID} in group ${group.id}`);
  const [tab] = group.tabs.splice(index, 1);
  if (group.activeTabID === tabID) {
    group.activeTabID = group.tabs[Math.min(index, group.tabs.length - 1)]?.id ?? null;
  }
  return tab;
}

function collapseIfEmpty(state, groupID) {
  const group = requireGroup(state, groupID);
  if (group.tabs.length > 0) return false;
  if (Object.keys(state.groups).length === 1 && state.root.type === "group") return false;

  const removed = removeGroupNode(state.root, groupID);
  if (!removed.found || !removed.node) throw new Error(`Cannot collapse group ${groupID}`);
  state.root = removed.node;
  delete state.groups[groupID];
  return true;
}

function removeGroupNode(node, groupID) {
  if (node.type === "group") {
    return node.groupID === groupID ? { found: true, node: null } : { found: false, node };
  }
  const first = removeGroupNode(node.first, groupID);
  if (first.found) {
    return { found: true, node: first.node ? { ...node, first: first.node } : node.second };
  }
  const second = removeGroupNode(node.second, groupID);
  if (second.found) {
    return { found: true, node: second.node ? { ...node, second: second.node } : node.first };
  }
  return { found: false, node };
}

function replaceGroupNode(node, groupID, replacement) {
  if (node.type === "group") {
    return node.groupID === groupID
      ? { found: true, node: replacement }
      : { found: false, node };
  }
  const first = replaceGroupNode(node.first, groupID, replacement);
  if (first.found) return { found: true, node: { ...node, first: first.node } };
  const second = replaceGroupNode(node.second, groupID, replacement);
  if (second.found) return { found: true, node: { ...node, second: second.node } };
  return { found: false, node };
}

function updateSplitFraction(node, splitID, fraction) {
  if (node.type === "group") return { found: false, node };
  if (node.id === splitID) return { found: true, node: { ...node, fraction }, fraction };
  const first = updateSplitFraction(node.first, splitID, fraction);
  if (first.found) return { found: true, node: { ...node, first: first.node }, fraction: first.fraction };
  const second = updateSplitFraction(node.second, splitID, fraction);
  if (second.found) return { found: true, node: { ...node, second: second.node }, fraction: second.fraction };
  return { found: false, node };
}

function walk(node, visitor) {
  visitor(node);
  if (node.type === "split") {
    walk(node.first, visitor);
    walk(node.second, visitor);
  }
}

function clampIndex(index, length) {
  return Math.max(0, Math.min(Number.isFinite(index) ? Math.trunc(index) : length, length));
}

function clampFraction(fraction) {
  return Math.max(MIN_FRACTION, Math.min(MAX_FRACTION, fraction));
}
