// A file tree from a flat, sorted list of paths: built once per list, then
// flattened into the rows in sight for whichever folders are open. Flattening
// only walks open folders, so a closed 50 000-file folder costs nothing.

export interface TreeFolder<T> {
  kind: "folder";
  /** `src/api` (no trailing slash); `""` for the root. */
  path: string;
  name: string;
  children: TreeNode<T>[];
  /** Every file below it (to count, or to roll a status up). */
  size: number;
}

export interface TreeLeaf<T> {
  kind: "file";
  path: string;
  name: string;
  item: T;
}

export type TreeNode<T> = TreeFolder<T> | TreeLeaf<T>;

/** One visible row: the node and how deep it sits (0 = top level). */
export interface TreeRow<T> {
  node: TreeNode<T>;
  depth: number;
  /** Its parent folder's path (`""` at the top). */
  parent: string;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

function sortNodes<T>(nodes: TreeNode<T>[]): void {
  // Folders first, then files; natural order ("file2" before "file10").
  nodes.sort((a, b) => (a.kind === b.kind ? collator.compare(a.name, b.name) : a.kind === "folder" ? -1 : 1));
  for (const node of nodes) if (node.kind === "folder") sortNodes(node.children);
}

/**
 * Builds the tree. A path ending with `/` is a folder with nothing listed in it
 * (git's ignored folders); every other path is a file. Folders are implied.
 */
export function buildTree<T>(items: T[], pathOf: (item: T) => string): TreeFolder<T> {
  const root: TreeFolder<T> = { kind: "folder", path: "", name: "", children: [], size: 0 };
  const folders = new Map<string, TreeFolder<T>>([["", root]]);

  const folderAt = (path: string): TreeFolder<T> => {
    const known = folders.get(path);
    if (known) return known;
    const cut = path.lastIndexOf("/");
    const parent = folderAt(cut < 0 ? "" : path.slice(0, cut));
    const folder: TreeFolder<T> = { kind: "folder", path, name: path.slice(cut + 1), children: [], size: 0 };
    parent.children.push(folder);
    folders.set(path, folder);
    return folder;
  };

  for (const item of items) {
    const raw = pathOf(item);
    if (raw.endsWith("/")) {
      folderAt(raw.slice(0, -1));
      continue;
    }
    const cut = raw.lastIndexOf("/");
    const parent = folderAt(cut < 0 ? "" : raw.slice(0, cut));
    parent.children.push({ kind: "file", path: raw, name: raw.slice(cut + 1), item });
    // Every folder above exists (`folderAt` made them): count the file in each.
    for (let p = parent.path; ; p = parentOf(p)) {
      const folder = folders.get(p);
      if (folder) folder.size++;
      if (p === "") break;
    }
  }
  sortNodes(root.children);
  return root;
}

/** `a/b/c` → `a/b`; `a` → `""`. */
export function parentOf(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut < 0 ? "" : path.slice(0, cut);
}

/** Every folder above `path` (`a/b/c.py` → `a`, `a/b`), to open the way to it. */
export function ancestorsOf(path: string): string[] {
  const parts = path.split("/");
  const out: string[] = [];
  for (let i = 1; i < parts.length; i++) out.push(parts.slice(0, i).join("/"));
  return out;
}

/** The rows in sight: open folders show their children, in order. */
export function flattenTree<T>(root: TreeFolder<T>, expanded: ReadonlySet<string>): TreeRow<T>[] {
  const rows: TreeRow<T>[] = [];
  const walk = (folder: TreeFolder<T>, depth: number) => {
    for (const node of folder.children) {
      rows.push({ node, depth, parent: folder.path });
      if (node.kind === "folder" && expanded.has(node.path)) walk(node, depth + 1);
    }
  };
  walk(root, 0);
  return rows;
}

/** Every folder of the tree (collapse all / expand all). */
export function allFolders<T>(root: TreeFolder<T>): string[] {
  const out: string[] = [];
  const walk = (folder: TreeFolder<T>) => {
    for (const node of folder.children) {
      if (node.kind === "folder") {
        out.push(node.path);
        walk(node);
      }
    }
  };
  walk(root);
  return out;
}
