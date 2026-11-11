// Suite tree: groups of items (role="tree"), for example courses and controls, or sheets and layers.
// From core ADR 0030 (freemapper-web), taken from freemapper-courseplan's editor without a change in
// behaviour. Served at {token}/suite/tree.js.
// Up and down move between the visible items, left and right close and open a group, Home and End jump to the
// first and last, Enter or Space selects. One item is in the tab order at a time; the current selection carries
// aria-selected and a marker sign. The tree's aria-label is the root's data-tree-label (a text, not an id).
// Open and closed groups are remembered by group id for the whole page.

const collapsed = new Set();

// groups: [{ id, label, items: [{ key, label }] }]. onSelect(key) is called for a chosen item.
export function buildTree(root, groups, onSelect) {
  const keep = root.querySelector("[tabindex='0']")?.dataset.key ?? null;
  const selected = root.querySelector("[aria-selected='true']")?.dataset.key ?? null;
  // A rebuild after an operation keeps the keyboard focus on the item it was on.
  const refocus = root.contains(document.activeElement) ? (document.activeElement.dataset?.key ?? null) : null;
  root.replaceChildren();
  const list = document.createElement("ul");
  list.setAttribute("role", "tree");
  list.className = "tree";
  list.setAttribute("aria-label", root.dataset.treeLabel ?? "");
  root.append(list);

  for (const group of groups) {
    const open = !collapsed.has(group.id);
    const head = document.createElement("li");
    head.setAttribute("role", "treeitem");
    head.setAttribute("aria-expanded", String(open));
    head.dataset.key = `group:${group.id}`;
    head.tabIndex = -1;
    const label = document.createElement("span");
    label.className = "item group";
    label.textContent = group.label;
    head.append(label);
    const children = document.createElement("ul");
    children.setAttribute("role", "group");
    children.hidden = !open;
    for (const item of group.items) {
      const li = document.createElement("li");
      li.setAttribute("role", "treeitem");
      li.setAttribute("aria-selected", String(item.key === selected));
      li.dataset.key = item.key;
      li.tabIndex = -1;
      const text = document.createElement("span");
      text.className = "item";
      text.textContent = item.label;
      li.append(text);
      children.append(li);
    }
    head.append(children);
    list.append(head);
  }

  const visible = () => [...list.querySelectorAll("[role='treeitem']")].filter((li) => !li.parentElement.hidden);
  const focusItem = (li) => {
    for (const other of list.querySelectorAll("[tabindex='0']")) other.tabIndex = -1;
    li.tabIndex = 0;
    li.focus();
  };
  const start = visible().find((li) => li.dataset.key === keep) ?? visible()[0];
  if (start) start.tabIndex = 0;
  const focused = refocus ? visible().find((li) => li.dataset.key === refocus) : null;
  if (focused) {
    for (const other of list.querySelectorAll("[tabindex='0']")) other.tabIndex = -1;
    focused.tabIndex = 0;
    focused.focus();
  }

  function toggle(head, open) {
    const id = head.dataset.key.slice("group:".length);
    if (open) collapsed.delete(id);
    else collapsed.add(id);
    head.setAttribute("aria-expanded", String(open));
    head.querySelector(":scope > ul").hidden = !open;
  }

  function activate(li) {
    if (li.dataset.key.startsWith("group:")) toggle(li, li.getAttribute("aria-expanded") !== "true");
    else onSelect(li.dataset.key);
  }

  list.addEventListener("click", (evt) => {
    const li = evt.target.closest("[role='treeitem']");
    if (!li) return;
    focusItem(li);
    activate(li);
  });

  list.addEventListener("keydown", (evt) => {
    const li = evt.target.closest("[role='treeitem']");
    if (!li || evt.target !== li) return;
    const items = visible();
    const at = items.indexOf(li);
    const isGroup = li.dataset.key.startsWith("group:");
    let handled = true;
    switch (evt.key) {
      case "ArrowDown": if (items[at + 1]) focusItem(items[at + 1]); break;
      case "ArrowUp": if (items[at - 1]) focusItem(items[at - 1]); break;
      case "Home": focusItem(items[0]); break;
      case "End": focusItem(items[items.length - 1]); break;
      case "ArrowRight":
        if (isGroup && li.getAttribute("aria-expanded") !== "true") toggle(li, true);
        else if (isGroup && items[at + 1]) focusItem(items[at + 1]);
        break;
      case "ArrowLeft":
        if (isGroup) toggle(li, false);
        else focusItem(li.parentElement.closest("[role='treeitem']"));
        break;
      case "Enter":
      case " ": activate(li); break;
      default: handled = false;
    }
    if (handled) evt.preventDefault();
  });

  return {
    setCurrent(key) {
      for (const li of list.querySelectorAll("[aria-selected]")) {
        li.setAttribute("aria-selected", String(li.dataset.key === key));
      }
    },
  };
}
