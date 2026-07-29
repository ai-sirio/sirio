import {
  activateTab,
  createInitialState,
  describeTopology,
  moveTab,
  resizeSplit,
  splitWithTab
} from "./layout-model.mjs";

const variants = {
  edge: {
    name: "Edge Preview",
    summary: "Edge Preview — the proposed half-pane appears directly beneath the pointer."
  },
  compass: {
    name: "Drop Compass",
    summary: "Drop Compass — an explicit five-direction control appears over the target pane."
  },
  rails: {
    name: "Insertion Rails",
    summary: "Insertion Rails — narrow gutters expose every legal split boundary."
  }
};

const params = new URLSearchParams(location.search);
const variant = variants[params.get("variant")] ? params.get("variant") : "edge";
const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
const paneLayout = document.querySelector("#pane-layout");

let state = createInitialState();
let pendingDrag = null;
let dragSession = null;
let resizeSession = null;
let suppressClickUntil = 0;

initialize();

function initialize() {
  document.documentElement.dataset.variant = variant;
  document.querySelector("#variant-summary").textContent = variants[variant].summary;
  document.querySelector("#state-variant").textContent = variants[variant].name;
  document.querySelectorAll("[data-variant-link]").forEach((link) => {
    link.classList.toggle("active", link.dataset.variantLink === variant);
  });
  document.querySelector("#reset-button").addEventListener("click", resetLayout);
  window.addEventListener("pointermove", onGlobalPointerMove, { passive: false });
  window.addEventListener("pointerup", onGlobalPointerUp);
  window.addEventListener("pointercancel", onGlobalPointerCancel);
  window.addEventListener("keydown", onGlobalKeyDown);
  window.addEventListener("blur", () => {
    if (dragSession) cancelDrag("Window focus changed — drag cancelled without mutation.");
    if (resizeSession) cancelResize("Window focus changed — resize cancelled.");
    pendingDrag = null;
  });
  window.__panePrototype = {
    getState: () => structuredClone(state),
    getVariant: () => variant,
    reset: resetLayout
  };
  render();
}

function resetLayout() {
  if (dragSession) cleanupDrag();
  if (resizeSession) cleanupResize();
  pendingDrag = null;
  state = createInitialState();
  render("Reset to the canonical three-group scenario.");
}

function render(message, messageKind) {
  paneLayout.replaceChildren(renderNode(state.root));
  renderInspector(message, messageKind);
}

function renderNode(node) {
  if (node.type === "group") return renderGroup(state.groups[node.groupID]);

  const split = document.createElement("div");
  split.className = `split-node ${node.axis}`;
  split.dataset.splitNode = node.id;

  const first = document.createElement("div");
  first.className = "split-child first";
  first.append(renderNode(node.first));
  setFirstChildBasis(first, node.fraction);

  const divider = document.createElement("div");
  divider.className = "divider";
  divider.dataset.splitId = node.id;
  divider.dataset.axis = node.axis;
  divider.setAttribute("role", "separator");
  divider.setAttribute("aria-label", `Resize ${node.axis} split`);
  divider.setAttribute("aria-valuenow", String(Math.round(node.fraction * 100)));
  divider.addEventListener("pointerdown", onDividerPointerDown);

  const second = document.createElement("div");
  second.className = "split-child second";
  second.append(renderNode(node.second));

  split.append(first, divider, second);
  return split;
}

function renderGroup(group) {
  const article = document.createElement("article");
  article.className = `pane-group${state.activeGroupID === group.id ? " active-group" : ""}`;
  article.dataset.groupId = group.id;
  article.style.viewTransitionName = `pane-group-${group.id}`;

  const tabbar = document.createElement("div");
  tabbar.className = "tabbar";
  tabbar.setAttribute("role", "tablist");
  tabbar.setAttribute("aria-label", `Pane group ${group.id} tabs`);

  group.tabs.forEach((tab) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `tab${group.activeTabID === tab.id ? " active" : ""}`;
    button.dataset.tabId = tab.id;
    button.dataset.groupId = group.id;
    button.setAttribute("role", "tab");
    button.setAttribute("aria-selected", String(group.activeTabID === tab.id));
    button.draggable = false;
    button.innerHTML = `<span class="tab-kind">${iconFor(tab.kind)}</span><span class="tab-title">${tab.title}</span><span class="tab-close">×</span>`;
    button.addEventListener("pointerdown", onTabPointerDown);
    button.addEventListener("click", () => onTabClick(group.id, tab.id));
    tabbar.append(button);
  });

  const addButton = document.createElement("button");
  addButton.type = "button";
  addButton.className = "add-tab";
  addButton.setAttribute("aria-label", "New tab — outside prototype scope");
  addButton.textContent = "+";
  const identity = document.createElement("span");
  identity.className = "group-identity";
  identity.textContent = `GROUP ${group.id}`;
  tabbar.append(addButton, identity);

  const activeTab = group.tabs.find((tab) => tab.id === group.activeTabID) ?? group.tabs[0];
  const content = document.createElement("div");
  content.className = `pane-content ${activeTab ? `${activeTab.kind}-content` : "empty-content"}`;
  content.innerHTML = activeTab ? contentFor(activeTab) : "Drop a tab here";

  article.append(tabbar, content);
  return article;
}

function iconFor(kind) {
  return { terminal: "›_", chat: "◌", code: "‹/›", markdown: "▤" }[kind] ?? "·";
}

function contentFor(tab) {
  if (tab.kind === "terminal") {
    return `<p><span class="prompt">~/tiller</span> git status --short</p><p class="terminal-muted">On branch pane-layout</p><p><span class="prompt">~/tiller</span> <span class="cursor"></span></p>`;
  }
  if (tab.kind === "chat") {
    return `<div class="chat-line">How should a dragged tab expose split targets?</div><div class="chat-line agent">Keep ownership local; preview topology before commit.</div>`;
  }
  if (tab.kind === "code") {
    return `<pre><code><em>struct</em> WorkspaceLayout {\n  <strong>var</strong> root: LayoutNode\n  <strong>var</strong> groups: [PaneGroupID: PaneGroup]\n  <strong>var</strong> activeGroupID: PaneGroupID\n}</code></pre>`;
  }
  return `<h2>Pane groups</h2><p>Local tabs, stable identity, direct manipulation, and transactional mutations.</p>`;
}

function onTabClick(groupID, tabID) {
  if (performance.now() < suppressClickUntil || dragSession) return;
  state = activateTab(state, { groupID, tabID });
  render();
}

function onTabPointerDown(event) {
  if (event.button !== 0 || pendingDrag || dragSession || resizeSession) return;
  const tabElement = event.currentTarget;
  const rect = tabElement.getBoundingClientRect();
  tabElement.setPointerCapture(event.pointerId);
  pendingDrag = {
    pointerId: event.pointerId,
    sourceElement: tabElement,
    sourceGroupID: tabElement.dataset.groupId,
    tabID: tabElement.dataset.tabId,
    sourceIndex: [...tabElement.parentElement.querySelectorAll(".tab")].indexOf(tabElement),
    startX: event.clientX,
    startY: event.clientY,
    grabX: event.clientX - rect.left,
    grabY: event.clientY - rect.top,
    width: rect.width,
    height: rect.height
  };
}

function onGlobalPointerMove(event) {
  if (resizeSession && event.pointerId === resizeSession.pointerId) {
    event.preventDefault();
    updateResize(event);
    return;
  }

  if (pendingDrag && event.pointerId === pendingDrag.pointerId) {
    const distance = Math.hypot(event.clientX - pendingDrag.startX, event.clientY - pendingDrag.startY);
    if (distance >= 4) beginDrag(event);
  }

  if (dragSession && event.pointerId === dragSession.pointerId) {
    event.preventDefault();
    updateDrag(event);
  }
}

function beginDrag(event) {
  const ghost = document.createElement("div");
  ghost.className = "drag-ghost";
  ghost.innerHTML = pendingDrag.sourceElement.innerHTML;
  ghost.style.width = `${pendingDrag.width}px`;
  document.body.append(ghost);

  dragSession = {
    ...pendingDrag,
    ghost,
    targetGroupID: null,
    zone: null,
    targetIndex: null
  };
  pendingDrag = null;
  dragSession.sourceElement.classList.add("is-drag-source");
  document.documentElement.dataset.dragging = "true";
  positionGhost(event.clientX, event.clientY);
  renderInspector("Tracking the original grab point. Release over a legal target to commit.");
}

function updateDrag(event) {
  positionGhost(event.clientX, event.clientY);
  const groupElement = groupAtPoint(event.clientX, event.clientY);
  if (!groupElement) {
    clearDropFeedback();
    dragSession.targetGroupID = null;
    dragSession.zone = null;
    dragSession.targetIndex = null;
    renderInspector("Outside the workspace — releasing here cancels.");
    return;
  }

  const groupID = groupElement.dataset.groupId;
  if (dragSession.targetGroupID !== groupID) {
    clearDropFeedback();
    dragSession.targetGroupID = groupID;
    installDropFeedback(groupElement);
  }

  let zone = resolveZone(groupElement, event.clientX, event.clientY);
  const sourceGroup = state.groups[dragSession.sourceGroupID];
  if (groupID === dragSession.sourceGroupID && sourceGroup.tabs.length === 1 && zone && zone !== "center") {
    zone = null;
  }

  dragSession.zone = zone;
  dragSession.targetIndex = zone === "center"
    ? insertionIndexFor(groupElement, event.clientX)
    : null;
  updateDropFeedback(groupElement, zone);
  updateInsertionIndicator(groupElement, dragSession.targetIndex, zone);
  renderInspector(
    zone
      ? `${labelForZone(zone)} in group ${groupID}. Release to commit.`
      : groupID === dragSession.sourceGroupID && sourceGroup.tabs.length === 1
        ? "The sole tab cannot split away from its own group."
        : "Move onto an explicit target before releasing."
  );
}

function positionGhost(x, y) {
  dragSession.ghost.style.left = `${x - dragSession.grabX}px`;
  dragSession.ghost.style.top = `${y - dragSession.grabY}px`;
}

function groupAtPoint(x, y) {
  return [...document.querySelectorAll(".pane-group")]
    .map((element) => ({ element, rect: element.getBoundingClientRect() }))
    .filter(({ rect }) => x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom)
    .sort((a, b) => a.rect.width * a.rect.height - b.rect.width * b.rect.height)[0]?.element ?? null;
}

function resolveZone(groupElement, x, y) {
  if (variant === "edge") return edgeZone(groupElement, x, y);
  const explicit = explicitControlZone(groupElement, x, y);
  if (explicit) return explicit;
  return variant === "rails" ? "center" : null;
}

function edgeZone(groupElement, x, y) {
  const rect = groupElement.getBoundingClientRect();
  const tabbar = groupElement.querySelector(".tabbar").getBoundingClientRect();
  if (y <= tabbar.bottom + 5) return "center";

  const distances = [
    ["left", (x - rect.left) / rect.width],
    ["right", (rect.right - x) / rect.width],
    ["top", (y - rect.top) / rect.height],
    ["bottom", (rect.bottom - y) / rect.height]
  ].sort((a, b) => a[1] - b[1]);
  return distances[0][1] <= 0.22 ? distances[0][0] : "center";
}

function explicitControlZone(groupElement, x, y) {
  for (const control of groupElement.querySelectorAll("[data-zone-control]")) {
    const rect = control.getBoundingClientRect();
    if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom) {
      return control.dataset.zoneControl;
    }
  }
  return null;
}

function installDropFeedback(groupElement) {
  const feedback = document.createElement("div");
  feedback.className = `drop-feedback ${variant}`;

  const preview = document.createElement("div");
  preview.className = "zone-preview";
  const label = document.createElement("div");
  label.className = "drop-label";
  label.textContent = "Choose a target";
  feedback.append(preview, label);

  if (variant === "compass") {
    const compass = document.createElement("div");
    compass.className = "compass-targets";
    for (const [zone, glyph] of [["top", "↑"], ["left", "←"], ["center", "●"], ["right", "→"], ["bottom", "↓"]]) {
      const control = document.createElement("i");
      control.className = "zone-control";
      control.dataset.zoneControl = zone;
      control.textContent = glyph;
      compass.append(control);
    }
    feedback.append(compass);
  }

  if (variant === "rails") {
    for (const zone of ["top", "right", "bottom", "left"]) {
      const control = document.createElement("i");
      control.className = "rail-control";
      control.dataset.zoneControl = zone;
      feedback.append(control);
    }
    const center = document.createElement("i");
    center.className = "rail-center";
    feedback.append(center);
  }

  groupElement.append(feedback);
}

function updateDropFeedback(groupElement, zone) {
  const feedback = groupElement.querySelector(".drop-feedback");
  if (!feedback) return;
  if (zone) feedback.dataset.dropZone = zone;
  else delete feedback.dataset.dropZone;
  feedback.querySelector(".drop-label").textContent = zone ? labelForZone(zone) : "Choose a target";
}

function clearDropFeedback() {
  document.querySelectorAll(".drop-feedback, .tab-insertion").forEach((element) => element.remove());
}

function insertionIndexFor(groupElement, x) {
  const tabs = [...groupElement.querySelectorAll(".tab")];
  const index = tabs.findIndex((tab) => {
    const rect = tab.getBoundingClientRect();
    return x < rect.left + rect.width / 2;
  });
  return index < 0 ? tabs.length : index;
}

function updateInsertionIndicator(groupElement, index, zone) {
  document.querySelectorAll(".tab-insertion").forEach((element) => element.remove());
  if (zone !== "center" || index === null) return;
  const tabbar = groupElement.querySelector(".tabbar");
  const tabs = [...tabbar.querySelectorAll(".tab")];
  const indicator = document.createElement("i");
  indicator.className = "tab-insertion";
  const left = index < tabs.length
    ? tabs[index].offsetLeft - 2
    : tabs.length
      ? tabs[tabs.length - 1].offsetLeft + tabs[tabs.length - 1].offsetWidth + 1
      : 7;
  indicator.style.left = `${left}px`;
  tabbar.append(indicator);
}

function labelForZone(zone) {
  return zone === "center" ? "Move into group" : `Split ${zone}`;
}

function onGlobalPointerUp(event) {
  if (resizeSession && event.pointerId === resizeSession.pointerId) {
    finishResize();
    return;
  }
  if (dragSession && event.pointerId === dragSession.pointerId) {
    finishDrag();
    return;
  }
  if (pendingDrag && event.pointerId === pendingDrag.pointerId) pendingDrag = null;
}

function finishDrag() {
  let next = null;
  let error = null;
  try {
    if (dragSession.targetGroupID && dragSession.zone === "center") {
      next = moveTab(state, {
        fromGroupID: dragSession.sourceGroupID,
        toGroupID: dragSession.targetGroupID,
        tabID: dragSession.tabID,
        toIndex: dragSession.targetIndex
      });
    } else if (dragSession.targetGroupID && dragSession.zone) {
      next = splitWithTab(state, {
        fromGroupID: dragSession.sourceGroupID,
        targetGroupID: dragSession.targetGroupID,
        tabID: dragSession.tabID,
        edge: dragSession.zone
      });
    }
  } catch (caught) {
    error = caught;
  }

  cleanupDrag();
  if (error) {
    render(`Drop rejected: ${error.message}`, "error");
  } else if (next) {
    commitState(next);
  } else {
    renderInspector("No legal target — layout left unchanged.");
  }
}

function cancelDrag(message = "Escape pressed — drag cancelled without mutation.") {
  cleanupDrag();
  renderInspector(message);
}

function cleanupDrag() {
  if (!dragSession) return;
  if (dragSession.sourceElement.hasPointerCapture?.(dragSession.pointerId)) {
    dragSession.sourceElement.releasePointerCapture(dragSession.pointerId);
  }
  dragSession.ghost.remove();
  dragSession.sourceElement.classList.remove("is-drag-source");
  clearDropFeedback();
  suppressClickUntil = performance.now() + 250;
  dragSession = null;
  pendingDrag = null;
  delete document.documentElement.dataset.dragging;
}

function commitState(next) {
  state = next;
  if (!reduceMotion && document.startViewTransition) {
    document.startViewTransition(() => render());
  } else {
    render();
  }
}

function onGlobalPointerCancel(event) {
  if (dragSession && event.pointerId === dragSession.pointerId) cancelDrag("Pointer cancelled — layout unchanged.");
  if (resizeSession && event.pointerId === resizeSession.pointerId) cancelResize("Pointer cancelled — size unchanged.");
  if (pendingDrag && event.pointerId === pendingDrag.pointerId) pendingDrag = null;
}

function onGlobalKeyDown(event) {
  if (event.key !== "Escape") return;
  if (dragSession) {
    event.preventDefault();
    cancelDrag();
  } else if (resizeSession) {
    event.preventDefault();
    cancelResize();
  } else {
    pendingDrag = null;
  }
}

function onDividerPointerDown(event) {
  if (event.button !== 0 || dragSession || resizeSession) return;
  const divider = event.currentTarget;
  const splitElement = divider.parentElement;
  divider.setPointerCapture(event.pointerId);
  divider.classList.add("is-resizing");
  resizeSession = {
    pointerId: event.pointerId,
    divider,
    splitElement,
    splitID: divider.dataset.splitId,
    axis: divider.dataset.axis,
    rect: splitElement.getBoundingClientRect(),
    fraction: null
  };
  renderInspector(`Resizing ${resizeSession.splitID}; release to store the preferred fraction.`);
  event.preventDefault();
}

function updateResize(event) {
  const { rect, axis } = resizeSession;
  const dividerHalf = 3;
  const total = axis === "horizontal" ? rect.width : rect.height;
  const coordinate = axis === "horizontal" ? event.clientX - rect.left : event.clientY - rect.top;
  const fraction = Math.max(0.18, Math.min(0.82, (coordinate - dividerHalf) / (total - 6)));
  resizeSession.fraction = fraction;
  const first = resizeSession.splitElement.querySelector(":scope > .split-child.first");
  setFirstChildBasis(first, fraction);
  resizeSession.divider.setAttribute("aria-valuenow", String(Math.round(fraction * 100)));
  renderInspector(`Live size ${Math.round(fraction * 100)}% / ${100 - Math.round(fraction * 100)}%.`);
}

function finishResize() {
  const next = resizeSession.fraction === null
    ? state
    : resizeSplit(state, { splitID: resizeSession.splitID, fraction: resizeSession.fraction });
  cleanupResize();
  state = next;
  render();
}

function cancelResize(message = "Escape pressed — resize cancelled.") {
  cleanupResize();
  render(message);
}

function cleanupResize() {
  if (!resizeSession) return;
  if (resizeSession.divider.hasPointerCapture?.(resizeSession.pointerId)) {
    resizeSession.divider.releasePointerCapture(resizeSession.pointerId);
  }
  resizeSession.divider.classList.remove("is-resizing");
  resizeSession = null;
}

function setFirstChildBasis(element, fraction) {
  element.style.flex = `0 0 calc(${fraction * 100}% - ${fraction * 6}px)`;
}

function renderInspector(message, messageKind) {
  const activeGroup = state.groups[state.activeGroupID];
  const activeTab = activeGroup?.tabs.find((tab) => tab.id === activeGroup.activeTabID);
  document.querySelector("#state-active-group").textContent = state.activeGroupID;
  document.querySelector("#state-dragging").textContent = dragSession
    ? `${dragSession.tabID} from ${dragSession.sourceGroupID}`
    : resizeSession
      ? resizeSession.splitID
      : "None";
  document.querySelector("#state-target").textContent = dragSession?.targetGroupID
    ? `${dragSession.targetGroupID} · ${dragSession.zone ?? "no zone"}`
    : "None";
  document.querySelector("#state-topology").textContent = describeTopology(state.root);
  document.querySelector("#state-mutation").textContent = state.lastMutation.label;
  document.querySelector("#state-summary").textContent = activeGroup
    ? `Group ${activeGroup.id} active · ${activeTab?.title ?? "No tab"} selected`
    : "No active group";

  const pill = document.querySelector("#drag-status-pill");
  pill.className = "status-pill";
  if (dragSession) {
    pill.textContent = "DRAGGING";
    pill.classList.add("dragging");
  } else if (resizeSession) {
    pill.textContent = "RESIZING";
    pill.classList.add("resizing");
  } else {
    pill.textContent = "IDLE";
  }

  const note = document.querySelector("#event-note");
  note.classList.toggle("error", messageKind === "error");
  note.textContent = message ?? "The ghost keeps the original grab point. A mutation commits only on a legal drop.";
}
