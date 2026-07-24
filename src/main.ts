import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";

interface LibraryLocation {
  path: string;
  drive: string;
  displayName: string;
}

interface PaperRecord {
  id: string;
  legacyId?: string;
  identitySource?: string;
  title: string;
  authors: string | null;
  year: number | null;
  collections: string[];
  path: string;
  relativePath: string;
  fileName: string;
  sizeBytes: number;
  modifiedAt: number | null;
  starred: boolean;
}

interface LibraryIndex {
  root: string;
  papers: PaperRecord[];
  collections: string[];
  stats: {
    paperCount: number;
    collectionCount: number;
    starredCount: number;
    totalBytes: number;
  };
  warnings: string[];
}

interface WorkspaceCollection {
  path: string;
  paperIds: string[];
}

interface WorkspaceCollectionIndex {
  version: number;
  initializedFrom: string;
  collections: WorkspaceCollection[];
}

interface WorkspaceDescriptor {
  root: string;
  name: string;
  version: number;
  paperpileRoot: string | null;
  paperpileMode: string;
}

interface CodexRuntimeStatus {
  available: boolean;
  version: string | null;
  command: string | null;
  running: boolean;
  sessionId: number | null;
  workspaceRoot: string | null;
}

interface CodexTerminalOutput {
  sessionId: number;
  data: string;
}

interface CodexTerminalExit {
  sessionId: number;
}

interface PresentedPaperList {
  title: string;
  description: string;
  papers: PresentedPaper[];
  updatedAt: number;
}

interface PresentedPaper {
  paperId?: string;
  title: string;
  authors: string;
  year: number | null;
  doi: string;
  url: string;
  note: string;
  status: string;
}

interface LibraryChangeToken {
  token: string;
  paperCount: number;
  checkedAt: number;
}

interface UpdateAuthStatus {
  configured: boolean;
  source: string | null;
  credentialStorageAvailable: boolean;
  detail?: string | null;
}

interface AppUpdateCheckResult {
  currentVersion: string;
  available: boolean;
  version: string | null;
  notes: string | null;
  publishedAt: string | null;
  authSource: string;
}

interface AppUpdateProgress {
  event: "started" | "progress" | "finished";
  data?: {
    contentLength?: number | null;
    downloaded?: number;
  };
}

interface ReviewCitation {
  paperId: string;
  title: string;
  authors: string;
  year: number | null;
  locator: string;
  note: string;
}

interface ReviewFigure {
  file: string;
  caption: string;
  sourcePaperId: string;
  page: number | null;
  addedAt: number;
}

interface ReviewSummary {
  id: string;
  theme: string;
  title: string;
  revision: number;
  createdAt: number;
  updatedAt: number;
  citationCount: number;
  figureCount: number;
}

interface ReviewDocument {
  id: string;
  theme: string;
  title: string;
  revision: number;
  createdAt: number;
  updatedAt: number;
  citations: ReviewCitation[];
  figures: ReviewFigure[];
  article: string;
  directory: string;
}

interface CollectionNode {
  name: string;
  path: string;
  source: "paperpile" | "workspace";
  collectionPath: string | null;
  paperIds: Set<string>;
  directPaperIds: Set<string>;
  children: Map<string, CollectionNode>;
}

type SortMode = "recent" | "title" | "year";
type MainMode = "library" | "reviews";
type ThemeMode = "system" | "light" | "dark";

const SIDEBAR_WIDTH = { min: 180, max: 420, initial: 228 };
const PAPER_LIST_WIDTH = { min: 260, max: 720, initial: 380 };

function storedWidth(key: string, bounds: { min: number; max: number; initial: number }): number {
  const stored = localStorage.getItem(key);
  if (stored === null) return bounds.initial;
  const value = Number(stored);
  return Number.isFinite(value) ? Math.min(bounds.max, Math.max(bounds.min, value)) : bounds.initial;
}

const app = document.querySelector<HTMLDivElement>("#app") as HTMLDivElement;
if (!app) throw new Error("App mount point was not found");
const isDemoMode = import.meta.env.DEV && new URLSearchParams(window.location.search).has("demo");

const state: {
  loading: boolean;
  loadingLabel: string;
  scanning: boolean;
  locations: LibraryLocation[];
  library: LibraryIndex | null;
  workspaceCollections: WorkspaceCollectionIndex | null;
  workspaceRoot: string | null;
  workspaceName: string | null;
  managedWorkspace: boolean;
  query: string;
  collection: string | null;
  starredOnly: boolean;
  sort: SortMode;
  mode: MainMode;
  selectedId: string | null;
  visibleLimit: number;
  error: string | null;
  collapsedCollections: Set<string>;
  creatingCollection: boolean;
  collectionSaving: boolean;
  theme: ThemeMode;
  sidebarCollapsed: boolean;
  sidebarWidth: number;
  listCollapsed: boolean;
  paperListWidth: number;
  reviewSummaries: ReviewSummary[];
  activeReview: ReviewDocument | null;
  reviewLoading: boolean;
  reviewSaving: boolean;
  reviewEditing: boolean;
  commandOpen: boolean;
  codexOpen: boolean;
  codexStarting: boolean;
  codexStatus: CodexRuntimeStatus | null;
  codexContextPaperId: string | null;
  codexError: string | null;
  codexPaperList: PresentedPaperList | null;
  codexPaperListVisible: boolean;
  codexPaperListSelection: number;
  libraryChangeToken: string | null;
  libraryLastCheckedAt: number | null;
  appVersion: string;
  updateAuth: UpdateAuthStatus | null;
  updateResult: AppUpdateCheckResult | null;
  updateDialogOpen: boolean;
  updateChecking: boolean;
  updateInstalling: boolean;
  updateDownloaded: number;
  updateContentLength: number | null;
  updateError: string | null;
} = {
  loading: true,
  loadingLabel: "Google Drive のマウントを探しています",
  scanning: false,
  locations: [],
  library: null,
  workspaceCollections: null,
  workspaceRoot: null,
  workspaceName: null,
  managedWorkspace: false,
  query: "",
  collection: null,
  starredOnly: false,
  sort: "recent",
  mode: "library",
  selectedId: null,
  visibleLimit: 120,
  error: null,
  collapsedCollections: new Set<string>(),
  creatingCollection: false,
  collectionSaving: false,
  theme: (localStorage.getItem("bukan.theme") as ThemeMode | null) ?? "system",
  sidebarCollapsed: false,
  sidebarWidth: storedWidth("bukan.sidebarWidth", SIDEBAR_WIDTH),
  listCollapsed: false,
  paperListWidth: storedWidth("bukan.paperListWidth", PAPER_LIST_WIDTH),
  reviewSummaries: [],
  activeReview: null,
  reviewLoading: false,
  reviewSaving: false,
  reviewEditing: false,
  commandOpen: false,
  codexOpen: false,
  codexStarting: false,
  codexStatus: null,
  codexContextPaperId: null,
  codexError: null,
  codexPaperList: null,
  codexPaperListVisible: false,
  codexPaperListSelection: 0,
  libraryChangeToken: null,
  libraryLastCheckedAt: null,
  appVersion: "0.2.2",
  updateAuth: null,
  updateResult: null,
  updateDialogOpen: false,
  updateChecking: false,
  updateInstalling: false,
  updateDownloaded: 0,
  updateContentLength: null,
  updateError: null,
};

const icons = {
  library: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 5.5A2.5 2.5 0 0 1 6.5 3H20v16H6.5A2.5 2.5 0 0 0 4 21.5v-16Z"/><path d="M4 18.5A2.5 2.5 0 0 1 6.5 16H20M8 7h8M8 11h6"/></svg>`,
  folder: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7.5h7l2-2h9v13H3v-11Z"/></svg>`,
  star: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m12 3 2.75 5.57 6.15.9-4.45 4.33 1.05 6.12L12 17.03l-5.5 2.89 1.05-6.12L3.1 9.47l6.15-.9L12 3Z"/></svg>`,
  search: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="6.5"/><path d="m16 16 4 4"/></svg>`,
  refresh: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 7v5h-5M4 17v-5h5"/><path d="M6.1 8.2A7 7 0 0 1 18.5 7M5.5 17A7 7 0 0 0 18 15.8"/></svg>`,
  external: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14 4h6v6M20 4l-9 9"/><path d="M18 13v7H4V6h7"/></svg>`,
  reveal: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M3 7.5h7l2-2h9v13H3v-11Z"/><path d="m14 11 3 2-3 2"/></svg>`,
  chevron: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m9 5 7 7-7 7"/></svg>`,
  drive: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m8.5 3-6 10.5L6 20h12l3.5-6.5L15.5 3h-7Z"/><path d="m8.5 3 6 10.5M2.5 13.5h12M18 20l-3.5-6.5"/></svg>`,
  moon: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 15.5A8 8 0 0 1 8.5 4 8.5 8.5 0 1 0 20 15.5Z"/></svg>`,
  sun: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3.5"/><path d="M12 2v2M12 20v2M4.93 4.93l1.42 1.42M17.65 17.65l1.42 1.42M2 12h2M20 12h2M4.93 19.07l1.42-1.42M17.65 6.35l1.42-1.42"/></svg>`,
  command: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M9 6H6a3 3 0 1 0 3 3V6Zm0 0v12H6a3 3 0 1 1 3-3M15 6h3a3 3 0 1 1-3 3V6Zm0 0v12h3a3 3 0 1 0-3-3"/></svg>`,
  panel: `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16"/></svg>`,
  list: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 6h12M8 12h12M8 18h12"/><circle cx="4" cy="6" r=".7"/><circle cx="4" cy="12" r=".7"/><circle cx="4" cy="18" r=".7"/></svg>`,
  settings: `<svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.83 2.83-.06-.06A1.7 1.7 0 0 0 15 19.4a1.7 1.7 0 0 0-1 .6 1.7 1.7 0 0 0-.4 1.1V21h-4v-.1A1.7 1.7 0 0 0 8.6 19.4a1.7 1.7 0 0 0-1.88.34l-.06.06-2.83-2.83.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-.6-1 1.7 1.7 0 0 0-1.1-.4H3v-4h.1A1.7 1.7 0 0 0 4.6 8.6a1.7 1.7 0 0 0-.34-1.88l-.06-.06 2.83-2.83.06.06A1.7 1.7 0 0 0 9 4.6a1.7 1.7 0 0 0 1-.6 1.7 1.7 0 0 0 .4-1.1V3h4v.1A1.7 1.7 0 0 0 15.4 4.6a1.7 1.7 0 0 0 1.88-.34l.06-.06 2.83 2.83-.06.06A1.7 1.7 0 0 0 19.4 9c.36.28.58.68.6 1.1v.1h1v4h-.1a1.7 1.7 0 0 0-1.5.8Z"/></svg>`,
  code: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m17 4-9.5 8L17 20l3-1.5v-13L17 4Z"/><path d="m7.5 12-4-3v6l4-3Z"/></svg>`,
  terminal: `<svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3M12 16h5"/></svg>`,
  review: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 3h11l3 3v15H5z"/><path d="M16 3v4h4M8 11h8M8 15h8M8 19h5"/></svg>`,
  close: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18"/></svg>`,
  context: `<svg viewBox="0 0 24 24" aria-hidden="true"><path d="M5 4h14v16H5zM8 8h8M8 12h8M8 16h5"/><path d="m17 14 4 2-4 2v-4Z"/></svg>`,
};

const codexTerminalHost = document.createElement("div");
codexTerminalHost.className = "codex-terminal-host";
const pdfPreviewHost = document.createElement("div");
pdfPreviewHost.className = "pdf-preview-host";
pdfPreviewHost.hidden = true;
document.body.append(pdfPreviewHost);
let pdfPreviewPaperId: string | null = null;
let pdfPreviewPath: string | null = null;
let pdfPreviewRevision = 0;
let pdfPreviewResizeObserver: ResizeObserver | null = null;
let codexTerminal: Terminal | null = null;
let codexFitAddon: FitAddon | null = null;
let codexResizeObserver: ResizeObserver | null = null;
let codexResizeTimer: number | null = null;
let codexBacklog = "";

function resolvedTheme(): "light" | "dark" {
  if (state.theme === "system") return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  return state.theme;
}

function applyTheme(): void {
  document.documentElement.dataset.theme = resolvedTheme();
  document.documentElement.dataset.themeMode = state.theme;
  if (codexTerminal) codexTerminal.options.theme = terminalTheme();
}

function terminalTheme() {
  return resolvedTheme() === "dark"
    ? {
        background: "#0c0d0f",
        foreground: "#f0f1f3",
        cursor: "#a5a6ff",
        cursorAccent: "#0c0d0f",
        selectionBackground: "#3d3d68",
        black: "#111214",
        red: "#e58c8c",
        green: "#91c7a5",
        yellow: "#d9be7a",
        blue: "#8b8cf8",
        magenta: "#bb9af7",
        cyan: "#7dcfff",
        white: "#d8dae0",
        brightBlack: "#686b73",
        brightWhite: "#ffffff",
      }
    : {
        background: "#fbfbfa",
        foreground: "#1c1d20",
        cursor: "#5b5bd6",
        cursorAccent: "#ffffff",
        selectionBackground: "#dadafa",
        black: "#1c1d20",
        red: "#a94444",
        green: "#397852",
        yellow: "#8a6820",
        blue: "#4f4fc4",
        magenta: "#7950a0",
        cyan: "#27758a",
        white: "#e3e3e0",
        brightBlack: "#686b73",
        brightWhite: "#ffffff",
      };
}

function setTheme(theme: ThemeMode): void {
  state.theme = theme;
  localStorage.setItem("bukan.theme", theme);
  applyTheme();
  render();
}

function cycleTheme(): void {
  const next: Record<ThemeMode, ThemeMode> = { system: "light", light: "dark", dark: "system" };
  setTheme(next[state.theme]);
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>'"]/g, (character) => {
    const entities: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      "'": "&#39;",
      '"': "&quot;",
    };
    return entities[character] ?? character;
  });
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const power = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** power;
  return `${new Intl.NumberFormat("ja-JP", { maximumFractionDigits: power > 1 ? 1 : 0 }).format(value)} ${units[power]}`;
}

function formatDate(timestamp: number | null): string {
  if (!timestamp) return "更新日不明";
  return new Intl.DateTimeFormat("ja-JP", {
    year: "numeric",
    month: "short",
    day: "numeric",
  }).format(new Date(timestamp));
}

function displayCollectionPath(collection: string): string {
  const parts = collection.split(" / ").map((part) => part.trim()).filter(Boolean);
  if (parts[0]?.toLocaleLowerCase() === "my papers") parts.shift();
  return parts.join(" / ");
}

function buildSourceCollectionTree(
  source: "paperpile" | "workspace",
  assignments: Array<{ path: string; paperIds: Iterable<string> }>,
  allPaperIds: Iterable<string>,
): CollectionNode {
  const sourceName = source === "paperpile" ? "Paperpile" : "Workspace";
  const allIds = new Set(allPaperIds);
  const root: CollectionNode = {
    name: sourceName,
    path: sourceName,
    source,
    collectionPath: null,
    paperIds: new Set(allIds),
    directPaperIds: new Set(),
    children: new Map(),
  };
  const all: CollectionNode = {
    name: "All",
    path: `${sourceName} / All`,
    source,
    collectionPath: null,
    paperIds: new Set(allIds),
    directPaperIds: new Set(),
    children: new Map(),
  };
  root.children.set("All", all);

  for (const assignment of assignments) {
    const displayPath = displayCollectionPath(assignment.path);
    if (!displayPath) continue;
    const assignedIds = new Set(assignment.paperIds);
    const parts = displayPath.split(" / ");
    let siblings = all.children;
    let accumulated = "";
    for (const part of parts) {
      accumulated = accumulated ? `${accumulated} / ${part}` : part;
      let node = siblings.get(part);
      if (!node) {
        node = {
          name: part,
          path: `${sourceName} / All / ${accumulated}`,
          source,
          collectionPath: accumulated,
          paperIds: new Set(),
          directPaperIds: new Set(),
          children: new Map(),
        };
        siblings.set(part, node);
      }
      assignedIds.forEach((paperId) => node?.paperIds.add(paperId));
      if (accumulated === displayPath) node.directPaperIds = new Set(assignedIds);
      siblings = node.children;
    }
  }

  const sortNodes = (nodes: Iterable<CollectionNode>): CollectionNode[] => [...nodes].sort((left, right) => left.name.localeCompare(right.name, "ja"));
  const finalize = (nodes: Iterable<CollectionNode>): CollectionNode[] => sortNodes(nodes).map((node) => {
    const children = finalize(node.children.values());
    node.children = new Map(children.map((child) => [child.name, child]));
    return node;
  });
  all.children = new Map(finalize(all.children.values()).map((node) => [node.name, node]));
  return root;
}

function buildCollectionTree(library: LibraryIndex): CollectionNode[] {
  const paperpileAssignments = new Map<string, Set<string>>();
  for (const paper of library.papers) {
    for (const originalCollection of paper.collections) {
      const displayPath = displayCollectionPath(originalCollection);
      if (!displayPath) continue;
      const paperIds = paperpileAssignments.get(displayPath) ?? new Set<string>();
      paperIds.add(paper.id);
      paperpileAssignments.set(displayPath, paperIds);
    }
  }
  const workspaceAssignments = state.workspaceCollections?.collections.map((collection) => ({
    path: collection.path,
    paperIds: collection.paperIds,
  })) ?? [];
  const allPaperIds = library.papers.map((paper) => paper.id);
  return [
    buildSourceCollectionTree(
      "paperpile",
      [...paperpileAssignments].map(([path, paperIds]) => ({ path, paperIds })),
      allPaperIds,
    ),
    buildSourceCollectionTree("workspace", workspaceAssignments, allPaperIds),
  ];
}

function collectionTreeItemTemplate(node: CollectionNode, depth = 0): string {
  const children = [...node.children.values()];
  const hasChildren = children.length > 0;
  const collapsed = state.collapsedCollections.has(node.path);
  const selectedPaperIsMember = !!state.selectedId && node.directPaperIds.has(state.selectedId);
  const membershipAction = node.source === "workspace" && node.collectionPath && state.selectedId
    ? `<button class="collection-membership ${selectedPaperIsMember ? "included" : ""}" type="button"
        data-workspace-membership="${escapeHtml(node.collectionPath)}" data-member="${selectedPaperIsMember ? "false" : "true"}"
        title="${selectedPaperIsMember ? "選択中の文献を外す" : "選択中の文献を追加"}"
        aria-label="${selectedPaperIsMember ? "選択中の文献を外す" : "選択中の文献を追加"}">${selectedPaperIsMember ? "✓" : "+"}</button>`
    : "";
  return `<div class="collection-node ${collapsed ? "collapsed" : ""}">
    <div class="collection-node-row ${state.collection === node.path ? "active" : ""} ${depth === 0 ? "collection-root-row" : ""}" style="--tree-depth:${depth}">
      ${hasChildren
        ? `<button class="collection-toggle" data-collection-toggle="${escapeHtml(node.path)}" aria-label="${collapsed ? "展開" : "折りたたむ"}" aria-expanded="${!collapsed}">${icons.chevron}</button>`
        : `<span class="collection-toggle-spacer"></span>`}
      <button class="collection-select" data-collection="${escapeHtml(node.path)}" title="${escapeHtml(node.path)}">
        ${icons.folder}<i>${escapeHtml(node.name)}</i><em>${node.paperIds.size}</em>
      </button>
      ${membershipAction}
    </div>
    ${hasChildren ? `<div class="collection-children">${children.map((child) => collectionTreeItemTemplate(child, depth + 1)).join("")}</div>` : ""}
  </div>`;
}

function countCollectionNodes(nodes: CollectionNode[]): number {
  return nodes.reduce((count, node) => count + 1 + countCollectionNodes([...node.children.values()]), 0);
}

function filteredPapers(): PaperRecord[] {
  if (!state.library) return [];
  const terms = state.query.trim().toLocaleLowerCase().split(/\s+/).filter(Boolean);
  const papers = state.library.papers.filter((paper) => {
    if (state.starredOnly && !paper.starred) return false;
    if (state.collection) {
      const parts = state.collection.split(" / ");
      const source = parts[0];
      const selectedPath = parts.slice(2).join(" / ");
      if (selectedPath && source === "Paperpile" && !paper.collections.some((collection) => {
        const displayPath = displayCollectionPath(collection);
        return displayPath === selectedPath || displayPath.startsWith(`${selectedPath} / `);
      })) return false;
      if (selectedPath && source === "Workspace") {
        const included = state.workspaceCollections?.collections.some((collection) =>
          (collection.path === selectedPath || collection.path.startsWith(`${selectedPath} / `))
          && collection.paperIds.includes(paper.id)
        );
        if (!included) return false;
      }
    }
    if (!terms.length) return true;
    const haystack = [
      paper.title,
      paper.authors ?? "",
      paper.year?.toString() ?? "",
      paper.fileName,
      ...paper.collections,
      ...(state.workspaceCollections?.collections
        .filter((collection) => collection.paperIds.includes(paper.id))
        .map((collection) => collection.path) ?? []),
    ].join(" ").toLocaleLowerCase();
    return terms.every((term) => haystack.includes(term));
  });

  return papers.sort((left, right) => {
    if (state.sort === "title") return left.title.localeCompare(right.title, "ja");
    if (state.sort === "year") {
      return (right.year ?? 0) - (left.year ?? 0) || left.title.localeCompare(right.title, "ja");
    }
    return (right.modifiedAt ?? 0) - (left.modifiedAt ?? 0) || left.title.localeCompare(right.title, "ja");
  });
}

function onboardingTemplate(): string {
  const discovered = state.locations.length
    ? `<div class="detected-list">
        ${state.locations.map((location) => `
          <button class="detected-drive" data-library-path="${escapeHtml(location.path)}">
            <span class="detected-icon">${icons.drive}</span>
            <span><strong>${escapeHtml(location.displayName)}</strong><small>${escapeHtml(location.path)}</small></span>
            <span class="round-arrow">${icons.chevron}</span>
          </button>`).join("")}
      </div>`
    : `<div class="not-found"><span class="status-dot"></span> マウント済みの Paperpile は見つかりませんでした</div>`;

  return `<main class="onboarding-shell">
    <button class="onboarding-version update-trigger" type="button">Bukan v${escapeHtml(state.appVersion)} · 更新を確認</button>
    <section class="onboarding-card">
      <div class="onboarding-mark"><span>B</span><i></i></div>
      <p class="eyebrow">LITERATURE WORKSPACE</p>
      <h1>文献を、読む場所へ。</h1>
      <p class="onboarding-copy">Paperpile のコレクションをそのままに、集めた PDF を横断して探し、読み進めるための静かなワークスペースです。</p>
      <div class="connection-panel">
        <div class="panel-heading"><span>Bukan ワークスペース</span><em>推奨</em></div>
        <div class="workspace-choices">
          <button class="workspace-choice open-workspace" type="button">
            <span>${icons.folder}</span><strong>ワークスペースを開く</strong><small>bukan.toml のあるフォルダ</small>
          </button>
          <button class="workspace-choice create-workspace" type="button">
            <span>+</span><strong>新しく初期化</strong><small>研究データを別の場所に作成</small>
          </button>
        </div>
      </div>
      <div class="connection-panel quick-preview-panel">
        <div class="panel-heading"><span>Paperpile を直接プレビュー</span><em>${state.locations.length ? `${state.locations.length} 件` : "未接続"}</em></div>
        ${discovered}
        <button class="secondary-button choose-folder" type="button">別の Paperpile フォルダを選ぶ</button>
      </div>
      ${state.error ? `<p class="inline-error">${escapeHtml(state.error)}</p>` : ""}
      <p class="read-only-note"><span>READ ONLY</span> Paperpile 内のファイルは変更しません</p>
    </section>
    <aside class="onboarding-aside" aria-hidden="true">
      <div class="folio folio-back"><span>RESEARCH NOTES</span></div>
      <div class="folio folio-front"><span>01</span><h2>Collected<br/>knowledge,<br/><i>in focus.</i></h2><div class="folio-lines"></div></div>
    </aside>
  </main>`;
}

function sidebarTemplate(): string {
  const library = state.library;
  if (!library) return "";
  const collectionTree = buildCollectionTree(library);
  const visibleCollectionCount = countCollectionNodes(collectionTree);

  return `<aside class="sidebar" aria-label="コレクション">
    <div class="sidebar-header"><span><strong>${escapeHtml(state.workspaceName ?? "文献プレビュー")}</strong><small>${library.stats.paperCount} papers</small></span><button class="sidebar-close" type="button" title="サイドバーを格納" aria-label="サイドバーを格納">${icons.panel}</button></div>
    <nav class="primary-nav" aria-label="ライブラリ">
      <p class="nav-label">ライブラリ</p>
      <button class="nav-item ${!state.collection && !state.starredOnly ? "active" : ""}" data-view="all">
        <span>${icons.library}すべての文献</span><em>${library.stats.paperCount}</em>
      </button>
      <button class="nav-item ${state.starredOnly ? "active" : ""}" data-view="starred">
        <span>${icons.star}スター付き</span><em>${library.stats.starredCount}</em>
      </button>
      ${state.codexPaperList ? `<button class="nav-item codex-list-nav ${state.codexPaperListVisible ? "active" : ""}" data-view="codex-list">
        <span>${icons.terminal}Codex リスト</span><em>${state.codexPaperList.papers.length}</em>
      </button>` : ""}
      <p class="nav-label collections-label"><span>コレクション</span><span class="collection-label-actions"><em>${visibleCollectionCount}</em><button class="new-workspace-collection" type="button" title="Workspaceコレクションを作成" aria-label="Workspaceコレクションを作成">+</button></span></p>
      ${state.creatingCollection ? `<form class="workspace-collection-form">
        <input id="workspace-collection-path" type="text" placeholder="例: レビュー / 手法" maxlength="512" autocomplete="off" />
        <button type="submit" ${state.collectionSaving ? "disabled" : ""}>作成</button>
        <button class="cancel-workspace-collection" type="button" aria-label="キャンセル">×</button>
      </form>` : ""}
      <div class="collection-list">
        ${collectionTree.map((node) => collectionTreeItemTemplate(node)).join("")}
      </div>
    </nav>
    <div class="sidebar-footer">
      <div class="drive-status"><span>${state.workspaceRoot && !state.managedWorkspace ? icons.folder : icons.drive}</span><div><strong>${escapeHtml(state.workspaceName ?? "Google Drive")}</strong><small>${escapeHtml(state.managedWorkspace ? library.root : state.workspaceRoot ?? library.root)}</small></div><i></i></div>
      <div class="workspace-footer-actions">
        ${state.workspaceRoot && !state.managedWorkspace ? `<button class="open-vscode" type="button">${icons.code}<span>VS Codeで開く</span></button>` : ""}
        <button class="change-library" type="button">ライブラリを変更</button>
      </div>
    </div>
    <div class="resize-handle sidebar-resize-handle" data-resize-pane="sidebar" role="separator" aria-orientation="vertical" aria-label="サイドバーの幅を変更" tabindex="0"></div>
  </aside>`;
}

function railTemplate(): string {
  const themeIcon = resolvedTheme() === "dark" ? icons.moon : icons.sun;
  const themeLabel = state.theme === "system" ? "System" : state.theme === "light" ? "Light" : "Dark";
  return `<aside class="app-rail" aria-label="アプリナビゲーション">
    <button class="rail-brand" type="button" data-rail-action="sidebar" title="コレクションを開閉" aria-label="コレクションを開閉">B</button>
    <nav class="rail-nav">
      <button class="rail-button ${state.mode === "library" ? "active" : ""}" type="button" data-rail-action="library" title="ライブラリ" aria-label="ライブラリ">${icons.library}</button>
      <button class="rail-button" type="button" data-rail-action="search" title="検索（/）" aria-label="検索">${icons.search}</button>
      <button class="rail-button ${state.mode === "reviews" ? "active" : ""}" type="button" data-rail-action="reviews" title="継続レビュー" aria-label="継続レビュー">${icons.review}</button>
      <button class="rail-button ${state.codexOpen ? "active" : ""}" type="button" data-rail-action="codex" title="Codex（Ctrl+J）" aria-label="Codexを開閉">${icons.terminal}</button>
    </nav>
    <div class="rail-bottom">
      <button class="rail-button" type="button" data-rail-action="command" title="コマンド（Ctrl+K）" aria-label="コマンドパレット">${icons.command}</button>
      <button class="rail-button theme-button" type="button" data-rail-action="theme" title="テーマ: ${themeLabel}" aria-label="テーマ: ${themeLabel}">${themeIcon}<span>${themeLabel.slice(0, 1)}</span></button>
    </div>
  </aside>`;
}

function commandPaletteTemplate(): string {
  if (!state.commandOpen) return "";
  const commands = [
    ["search", icons.search, "論文を検索", "現在のライブラリ"],
    ["all", icons.library, "すべての文献", "ライブラリ"],
    ["starred", icons.star, "スター付き文献", "ライブラリ"],
    ["reviews", icons.review, "継続レビューを開く", `${state.reviewSummaries.length} themes`],
    ...(state.workspaceRoot ? [["toggle-codex", icons.terminal, state.codexOpen ? "Codexを閉じる" : "Codexを開く", "現在のライブラリ"]] : []),
    ...(state.workspaceRoot && !state.managedWorkspace ? [["open-vscode", icons.code, "VS Codeでワークスペースを開く", "ワークスペース"]] : []),
    ["toggle-sidebar", icons.panel, state.sidebarCollapsed ? "コレクションを表示" : "コレクションを格納", "表示"],
    ["toggle-list", icons.list, state.listCollapsed ? "論文一覧を表示" : "論文一覧を格納", "表示"],
    ["theme-system", icons.settings, "テーマ: System", "表示"],
    ["theme-light", icons.sun, "テーマ: Light", "表示"],
    ["theme-dark", icons.moon, "テーマ: Dark", "表示"],
    ["refresh", icons.refresh, "ライブラリを再読み込み", "データ"],
    ["check-update", icons.refresh, "Bukanの更新を確認", `App v${state.appVersion}`],
  ];
  return `<div class="command-backdrop" role="presentation">
    <section class="command-palette" role="dialog" aria-modal="true" aria-label="コマンドパレット">
      <label class="command-search">${icons.search}<input id="command-input" type="search" placeholder="検索またはコマンドを入力…" autocomplete="off" /></label>
      <div class="command-results">
        ${commands.map(([action, icon, label, group], index) => `<button class="command-item ${index === 0 ? "keyboard-active" : ""}" type="button" data-command-action="${action}" data-command-search="${escapeHtml(`${label} ${group}`.toLocaleLowerCase())}">${icon}<span><strong>${label}</strong><small>${group}</small></span>${index === 0 ? "<kbd>/</kbd>" : ""}</button>`).join("")}
        <p class="command-empty">一致するコマンドがありません</p>
      </div>
      <footer><span><kbd>↑</kbd><kbd>↓</kbd> 移動</span><span><kbd>Esc</kbd> 閉じる</span></footer>
    </section>
  </div>`;
}

function paperListTemplate(papers: PaperRecord[]): string {
  const visible = papers.slice(0, state.visibleLimit);
  const viewTitle = state.starredOnly ? "スター付き" : state.collection ?? "すべての文献";
  return `<section class="paper-column">
    <div class="paper-column-header">
      <div><p class="section-kicker">COLLECTION</p><h1>${escapeHtml(viewTitle)}</h1></div>
      <span class="result-count">${papers.length} papers</span>
    </div>
    <div class="search-row">
      <label class="search-box">${icons.search}<input id="search-input" type="search" placeholder="タイトル、著者、年で検索" value="${escapeHtml(state.query)}" autocomplete="off" /></label>
      <select id="sort-select" aria-label="並び順">
        <option value="recent" ${state.sort === "recent" ? "selected" : ""}>最近の更新</option>
        <option value="title" ${state.sort === "title" ? "selected" : ""}>タイトル</option>
        <option value="year" ${state.sort === "year" ? "selected" : ""}>出版年</option>
      </select>
    </div>
    <div class="paper-list">
      ${visible.length ? visible.map(paperRowTemplate).join("") : `<div class="empty-results"><span>${icons.search}</span><h3>該当する文献がありません</h3><p>検索語やコレクションを変えてみてください。</p></div>`}
      ${papers.length > visible.length ? `<button class="load-more" type="button">さらに表示 <span>${visible.length} / ${papers.length}</span></button>` : ""}
    </div>
    <div class="resize-handle paper-list-resize-handle" data-resize-pane="paper-list" role="separator" aria-orientation="vertical" aria-label="文献リストの幅を変更" tabindex="0"></div>
  </section>`;
}

function paperRowTemplate(paper: PaperRecord): string {
  const meta = [paper.authors, paper.year?.toString()].filter(Boolean).join(" · ") || "書誌情報なし";
  const collection = displayCollectionPath(paper.collections[0] ?? "") || "未分類";
  return `<button class="paper-row ${state.selectedId === paper.id ? "selected" : ""}" data-paper-id="${paper.id}" type="button">
    <span class="paper-accent"></span>
    <span class="paper-copy">
      <span class="paper-title">${escapeHtml(paper.title)}</span>
      <span class="paper-meta">${escapeHtml(meta)}</span>
      <span class="paper-tags"><i>${escapeHtml(collection)}</i>${paper.collections.length > 1 ? `<em>+${paper.collections.length - 1}</em>` : ""}</span>
    </span>
    <span class="paper-side">${paper.starred ? `<i class="starred">${icons.star}</i>` : ""}<small>${formatBytes(paper.sizeBytes)}</small></span>
  </button>`;
}

function pdfPreviewMarkup(paper: PaperRecord): string {
  if ("__TAURI_INTERNALS__" in window) {
    const previewUrl = `${convertFileSrc(paper.path)}#pagemode=bookmarks`;
    return `<object class="pdf-object" data="${escapeHtml(previewUrl)}" type="application/pdf">
      <div class="pdf-fallback"><p>PDF プレビューを表示できませんでした。</p><button class="action-button persistent-open-external">${icons.external}既定のアプリで開く</button></div>
    </object>
    <div class="pdf-loading" role="status">
      <span class="reading-spinner"><i></i><i></i><i></i></span>
      <strong>PDF を読み込んでいます</strong>
      <small>Google Drive 上のファイルを準備しています</small>
    </div>`;
  }
  return `<div class="demo-pdf-page" aria-label="PDF preview placeholder">
    <small>BUKAN · READING PREVIEW</small><h3>${escapeHtml(paper.title)}</h3>
    <p>${escapeHtml(paper.authors ?? "")}${paper.year ? ` · ${paper.year}` : ""}</p>
    <i></i><i></i><i></i><i></i><i></i><i></i><i></i><i></i>
  </div>`;
}

function updatePdfPreview(paper: PaperRecord): void {
  if (pdfPreviewPaperId === paper.id && pdfPreviewPath === paper.path) return;
  pdfPreviewPaperId = paper.id;
  pdfPreviewPath = paper.path;
  pdfPreviewRevision += 1;
  pdfPreviewHost.dataset.previewPaperId = paper.id;
  pdfPreviewHost.dataset.revision = String(pdfPreviewRevision);
  pdfPreviewHost.innerHTML = pdfPreviewMarkup(paper);
  pdfPreviewHost.querySelector<HTMLElement>(".persistent-open-external")?.addEventListener("click", () => {
    void runPaperAction("open_paper");
  });
  const pdfObject = pdfPreviewHost.querySelector<HTMLObjectElement>(".pdf-object");
  const pdfLoading = pdfPreviewHost.querySelector<HTMLElement>(".pdf-loading");
  if (!pdfObject || !pdfLoading) return;
  pdfObject.addEventListener("load", () => pdfLoading.classList.add("loaded"), { once: true });
  pdfObject.addEventListener("error", () => {
    pdfLoading.classList.add("failed");
    const title = pdfLoading.querySelector("strong");
    const detail = pdfLoading.querySelector("small");
    if (title) title.textContent = "プレビューを読み込めませんでした";
    if (detail) detail.textContent = "「別ウィンドウで開く」をお試しください";
  }, { once: true });
}

function attachPdfPreview(): void {
  const mount = document.querySelector<HTMLElement>(".pdf-preview-mount");
  const paper = state.library?.papers.find((candidate) => candidate.id === state.selectedId);
  pdfPreviewResizeObserver?.disconnect();
  if (!mount || !paper) {
    pdfPreviewHost.hidden = true;
    return;
  }
  updatePdfPreview(paper);
  pdfPreviewHost.hidden = false;
  const positionPreview = () => {
    if (!mount.isConnected || pdfPreviewHost.hidden) return;
    const bounds = mount.getBoundingClientRect();
    pdfPreviewHost.style.left = `${bounds.left}px`;
    pdfPreviewHost.style.top = `${bounds.top}px`;
    pdfPreviewHost.style.width = `${bounds.width}px`;
    pdfPreviewHost.style.height = `${bounds.height}px`;
  };
  positionPreview();
  pdfPreviewResizeObserver = new ResizeObserver(positionPreview);
  pdfPreviewResizeObserver.observe(mount);
  window.requestAnimationFrame(positionPreview);
}

function viewerTemplate(): string {
  const paper = state.library?.papers.find((candidate) => candidate.id === state.selectedId);
  if (!paper) {
    return `<section class="viewer empty-viewer">
      <div class="empty-document"><span>PDF</span><i></i><i></i><i></i></div>
      <p class="section-kicker">READING DESK</p>
      <h2>文献を選択して<br/>読み始める</h2>
      <p>左の一覧から文献を選ぶと、ここに PDF と書誌情報を表示します。</p>
    </section>`;
  }

  return `<section class="viewer">
    <header class="viewer-header">
      <div class="viewer-title-wrap">
        <p class="section-kicker">NOW READING</p>
        <h2 title="${escapeHtml(paper.title)}">${escapeHtml(paper.title)}</h2>
        <p>${escapeHtml([paper.authors, paper.year?.toString()].filter(Boolean).join(" · ") || paper.fileName)}</p>
      </div>
      <div class="viewer-actions">
        ${state.workspaceRoot ? `<button class="icon-button set-codex-context ${state.codexContextPaperId === paper.id ? "context-active" : ""}" title="この論文をCodexのコンテキストに設定" aria-label="この論文をCodexのコンテキストに設定">${icons.context}</button>` : ""}
        <button class="icon-button reveal-paper" title="エクスプローラーで表示" aria-label="エクスプローラーで表示">${icons.reveal}</button>
        <button class="action-button open-external">${icons.external}<span>別ウィンドウで開く</span></button>
      </div>
    </header>
    <div class="paper-detail-strip">
      <span><small>COLLECTION</small><strong>${escapeHtml(paper.collections.map(displayCollectionPath).filter(Boolean).join(" · ") || "未分類")}</strong></span>
      <span><small>UPDATED</small><strong>${formatDate(paper.modifiedAt)}</strong></span>
      <span><small>FILE SIZE</small><strong>${formatBytes(paper.sizeBytes)}</strong></span>
      ${paper.starred ? `<span class="detail-star">${icons.star}<strong>STARRED</strong></span>` : ""}
    </div>
    <div class="pdf-stage">
      <div class="pdf-preview-mount"></div>
    </div>
  </section>`;
}

function normalizedPaperTitle(title: string): string {
  return title.toLocaleLowerCase().normalize("NFKC").replace(/[^\p{L}\p{N}]+/gu, " ").trim();
}

function matchingLibraryPaper(paper: PresentedPaper): PaperRecord | null {
  if (paper.paperId) {
    const byId = state.library?.papers.find((candidate) =>
      candidate.id === paper.paperId || candidate.legacyId === paper.paperId
    );
    if (byId) return byId;
  }
  const normalized = normalizedPaperTitle(paper.title);
  return state.library?.papers.find((candidate) => normalizedPaperTitle(candidate.title) === normalized) ?? null;
}

function codexPaperListTemplate(): string {
  const list = state.codexPaperList;
  if (!list) return viewerTemplate();
  return `<section class="viewer codex-list-viewer">
    <header class="viewer-header codex-list-header">
      <div class="viewer-title-wrap">
        <p class="section-kicker">CODEX PAPER LIST</p>
        <h2>${escapeHtml(list.title)}</h2>
        <p>${list.papers.length} papers · 一時リスト</p>
      </div>
      <div class="viewer-actions">
        <button class="codex-text-button persist-codex-paper-list" data-list-destination="reports" type="button">レポート保存</button>
        <button class="codex-text-button persist-codex-paper-list" data-list-destination="candidates" type="button">候補化</button>
        <button class="codex-text-button clear-codex-paper-list" type="button">消去</button>
        <button class="icon-button close-codex-paper-list" type="button" title="PDFへ戻る" aria-label="PDFへ戻る">${icons.close}</button>
      </div>
    </header>
    ${list.description ? `<div class="codex-list-description">${escapeHtml(list.description)}</div>` : ""}
    <div class="codex-presented-list">
      ${list.papers.map((paper, index) => {
        const libraryPaper = matchingLibraryPaper(paper);
        const metadata = [paper.authors, paper.year?.toString(), paper.doi ? `doi:${paper.doi}` : ""].filter(Boolean).join(" · ");
        return `<article class="codex-presented-paper ${state.codexPaperListSelection === index ? "selected" : ""}">
          <button class="codex-paper-select" type="button" data-codex-paper-index="${index}">
            <span class="paper-accent"></span>
            <span><small>${escapeHtml(paper.status || (libraryPaper ? "IN LIBRARY" : "CANDIDATE"))}</small><strong>${escapeHtml(paper.title)}</strong><i>${escapeHtml(metadata || "書誌情報なし")}</i></span>
          </button>
          ${paper.note ? `<p>${escapeHtml(paper.note)}</p>` : ""}
          <footer>
            ${libraryPaper ? `<button type="button" data-open-library-paper="${libraryPaper.id}">ライブラリで開く</button>` : ""}
            ${paper.url ? `<span title="${escapeHtml(paper.url)}">${escapeHtml(paper.url)}</span>` : ""}
          </footer>
        </article>`;
      }).join("")}
    </div>
  </section>`;
}

function codexPaneTemplate(): string {
  const contextPaper = state.library?.papers.find((paper) => paper.id === state.codexContextPaperId);
  const status = state.codexStatus;
  const version = status?.version?.replace(/^codex-cli\s*/i, "") ?? "CLI";
  let body = "";

  if (!state.workspaceRoot) {
    body = `<div class="codex-empty">
      <span>${icons.terminal}</span>
      <h3>Codexの作業領域を準備できません</h3>
      <p>ライブラリを開き直すと、Appが作業領域を自動的に準備します。</p>
    </div>`;
  } else if (state.codexStarting || !status) {
    body = `<div class="codex-empty">
      <span class="reading-spinner"><i></i><i></i><i></i></span>
      <h3>${state.codexStarting ? "Codexを起動しています" : "Codexを確認しています"}</h3>
      <p>ライブラリ用の作業領域とローカルのCodex CLIを準備しています。</p>
    </div>`;
  } else if (!status.available) {
    body = `<div class="codex-empty">
      <span>${icons.terminal}</span>
      <h3>Codex CLIが見つかりません</h3>
      <p>Codex CLIをインストールすると、ここに生のCodex TUIが表示されます。</p>
      <code>npm install -g @openai/codex</code>
      <button class="secondary-button retry-codex" type="button">再確認</button>
    </div>`;
  } else if (!status.running) {
    body = `<div class="codex-empty">
      <span>${icons.terminal}</span>
      <h3>Codexセッションは終了しました</h3>
      <p>${state.codexError ? escapeHtml(state.codexError) : "同じライブラリで新しいセッションを開始できます。"}</p>
      <button class="action-button start-codex" type="button">${icons.terminal}Codexを起動</button>
    </div>`;
  } else {
    body = `<div class="codex-terminal-mount" aria-label="Codex terminal"></div>`;
  }

  return `<aside class="codex-pane">
    <header class="codex-header">
      <div><span class="codex-mark">${icons.terminal}</span><p><strong>Codex</strong><small>${escapeHtml(version)} · embedded PowerShell</small></p></div>
      <div class="codex-header-actions">
        ${status?.running ? `<button class="codex-text-button stop-codex" type="button" title="Codexを終了">終了</button>` : ""}
        <button class="icon-button close-codex" type="button" title="Codexペインを閉じる" aria-label="Codexペインを閉じる">${icons.close}</button>
      </div>
    </header>
    <div class="codex-context-bar">
      <span>CONTEXT</span>
      ${contextPaper
        ? `<button type="button" class="context-chip" title="${escapeHtml(contextPaper.title)}">${icons.context}<i>${escapeHtml(contextPaper.title)}</i></button>`
        : `<p>Viewerの文献を選び、${icons.context}<span>で対象に設定</span></p>`}
      ${contextPaper ? `<button class="clear-codex-context" type="button">Clear</button>` : ""}
    </div>
    <div class="codex-body">
      ${state.codexError && status?.running ? `<div class="codex-inline-error">${escapeHtml(state.codexError)}</div>` : ""}
      <div class="codex-body-content">${body}</div>
    </div>
  </aside>`;
}

function reviewInlineTemplate(value: string): string {
  return escapeHtml(value)
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/(\[@[A-Za-z0-9_-]+(?:,[^\]]+)?\])/g, '<span class="review-citation-marker">$1</span>');
}

function reviewArticleTemplate(review: ReviewDocument): string {
  const lines = review.article.split(/\r?\n/);
  return lines.map((line) => {
    const image = line.match(/^!\[(.*)\]\(figures\/([^)]+)\)$/);
    if (image) {
      const figure = review.figures.find((candidate) => candidate.file === image[2]);
      if (figure) {
        const imagePath = `${review.directory}\\figures\\${figure.file}`;
        return `<figure><img src="${escapeHtml(convertFileSrc(imagePath))}" alt="${escapeHtml(image[1] ?? figure.caption)}" /><figcaption>${escapeHtml(figure.caption)}${figure.page ? ` · p. ${figure.page}` : ""}</figcaption></figure>`;
      }
    }
    const heading = line.match(/^(#{1,3})\s+(.+)$/);
    if (heading) {
      const level = heading[1]?.length ?? 1;
      return `<h${level}>${reviewInlineTemplate(heading[2] ?? "")}</h${level}>`;
    }
    if (line.startsWith("> ")) return `<blockquote>${reviewInlineTemplate(line.slice(2))}</blockquote>`;
    if (line.startsWith("- ")) return `<p class="review-bullet"><span>•</span>${reviewInlineTemplate(line.slice(2))}</p>`;
    if (!line.trim()) return `<div class="review-paragraph-break"></div>`;
    return `<p>${reviewInlineTemplate(line)}</p>`;
  }).join("");
}

function reviewsTemplate(): string {
  const review = state.activeReview;
  return `<section class="reviews-view">
    <aside class="reviews-index">
      <header><p class="section-kicker">LIVING REVIEWS</p><h1>継続レビュー</h1><small>テーマ別に引用と図を蓄積</small></header>
      <div class="review-codex-guide">
        <p>テーマ作成、文献の引用、図の添付はCodexから行います。</p>
        <button class="action-button open-review-codex" type="button">${icons.terminal}Codexを開く</button>
      </div>
      <div class="reviews-list">
        ${state.reviewSummaries.length
          ? state.reviewSummaries.map((item) => `<button class="review-list-item ${review?.id === item.id ? "active" : ""}" type="button" data-review-id="${escapeHtml(item.id)}">
              <strong>${escapeHtml(item.title)}</strong>
              <span>rev.${item.revision} · 引用 ${item.citationCount} · 図 ${item.figureCount}</span>
              <small>${new Intl.DateTimeFormat("ja-JP", { dateStyle: "medium" }).format(new Date(item.updatedAt * 1000))}</small>
            </button>`).join("")
          : `<div class="reviews-empty-list"><span>${icons.review}</span><p>Codexへテーマを伝えると、MCP経由でレビューがここへ作成されます。</p></div>`}
      </div>
    </aside>
    <article class="review-document">
      ${state.reviewLoading
        ? `<div class="review-document-empty"><span class="reading-spinner"><i></i><i></i><i></i></span><p>レビューを読み込んでいます</p></div>`
        : !review
          ? `<div class="review-document-empty">${icons.review}<h2>テーマを選択してください</h2><p>本文はMarkdown、引用は論文IDとページ位置、画像は出典付きの図として保存されます。</p></div>`
          : `<header class="review-document-header">
              <div><p class="section-kicker">REVISION ${review.revision}</p><h2>${escapeHtml(review.title)}</h2><small>${escapeHtml(review.theme)}</small></div>
              <div>
                <button class="secondary-update-action review-with-codex" type="button">${icons.terminal}Codexで更新</button>
                ${state.reviewEditing
                  ? `<button class="secondary-update-action cancel-review-edit" type="button">キャンセル</button><button class="action-button save-review" type="button" ${state.reviewSaving ? "disabled" : ""}>${state.reviewSaving ? "保存中" : "改訂を保存"}</button>`
                  : `<button class="secondary-update-action edit-review" type="button">本文を編集</button>`}
              </div>
            </header>
            <div class="review-document-body">
              ${state.reviewEditing
                ? `<textarea id="review-article-editor" spellcheck="true">${escapeHtml(review.article)}</textarea>`
                : `<div class="review-article">${reviewArticleTemplate(review)}</div>`}
              <aside class="review-evidence">
                <section><h3>引用文献 <span>${review.citations.length}</span></h3>
                  ${review.citations.length
                    ? review.citations.map((citation) => `<article><strong>${escapeHtml(citation.title)}</strong><p>${escapeHtml([citation.authors, citation.year?.toString()].filter(Boolean).join(" · "))}</p><small>${escapeHtml(citation.locator || "位置未記録")}${citation.note ? ` · ${escapeHtml(citation.note)}` : ""}</small></article>`).join("")
                    : `<p>Codexから本文を更新するときに、論文IDとPDFページを構造化して記録できます。</p>`}
                </section>
                <section><h3>図 <span>${review.figures.length}</span></h3>
                  ${review.figures.length
                    ? review.figures.map((figure) => `<figure><img src="${escapeHtml(convertFileSrc(`${review.directory}\\figures\\${figure.file}`))}" alt="${escapeHtml(figure.caption)}" /><figcaption>${escapeHtml(figure.caption)}${figure.page ? ` · p. ${figure.page}` : ""}</figcaption></figure>`).join("")
                    : `<p>CodexがWorkspace内へ抽出した画像を、出典論文・ページ付きで添付できます。</p>`}
                </section>
              </aside>
            </div>`}
    </article>
  </section>`;
}

function workspaceTemplate(): string {
  const papers = filteredPapers();
  const currentTitle = state.mode === "reviews"
    ? state.activeReview?.title ?? "継続レビュー"
    : state.collection ?? (state.starredOnly ? "Starred Papers" : "All Papers");
  return `<div class="workspace ${state.sidebarCollapsed ? "sidebar-collapsed" : ""}" style="--sidebar-width:${state.sidebarWidth}px;--paper-list-width:${state.paperListWidth}px">
    ${railTemplate()}
    ${sidebarTemplate()}
    <main class="workspace-main">
      <header class="topbar">
        <div class="breadcrumb"><span>${escapeHtml(state.workspaceName ?? "Paperpile")}</span>${icons.chevron}<strong>${escapeHtml(currentTitle)}</strong></div>
        <div class="topbar-actions">
          ${isDemoMode ? `<span class="demo-badge">DEMO DATA · 4件のみ</span>` : ""}
          ${state.scanning ? `<span class="scan-status"><i></i>ライブラリを読み取り中</span>` : ""}
          ${!state.scanning && state.libraryLastCheckedAt ? `<span class="watch-status"><i></i>Paperpile 監視中</span>` : ""}
          <button class="update-trigger update-status-button ${state.updateResult?.available ? "available" : ""}" type="button">
            ${state.updateChecking ? `<i></i>確認中` : state.updateResult?.available ? `v${escapeHtml(state.updateResult.version ?? "")} 更新` : `v${escapeHtml(state.appVersion)}`}
          </button>
          ${state.library?.warnings.length ? `<span class="warning-count" title="読み込めなかったファイルがあります">${state.library.warnings.length} warnings</span>` : ""}
          ${state.codexPaperList ? `<button class="codex-list-badge" type="button">${icons.terminal}<span>${escapeHtml(state.codexPaperList.title)}</span><em>${state.codexPaperList.papers.length}</em></button>` : ""}
          <button class="command-trigger" type="button">${icons.search}<span>検索とコマンド</span><kbd>Ctrl K</kbd></button>
          ${state.mode === "library" ? `<button class="icon-button toggle-list" title="論文一覧を${state.listCollapsed ? "表示" : "格納"}" aria-label="論文一覧を${state.listCollapsed ? "表示" : "格納"}">${icons.list}</button>` : ""}
          <button class="icon-button refresh-library ${state.scanning ? "spinning" : ""}" title="再読み込み" aria-label="再読み込み">${icons.refresh}</button>
        </div>
      </header>
      ${state.mode === "reviews"
        ? reviewsTemplate()
        : `<div class="content-grid ${state.listCollapsed ? "list-collapsed" : ""} ${state.codexOpen ? "codex-open" : ""}">${paperListTemplate(papers)}${state.codexPaperListVisible ? codexPaperListTemplate() : viewerTemplate()}${state.codexOpen ? codexPaneTemplate() : ""}</div>`}
    </main>
    ${commandPaletteTemplate()}
  </div>`;
}

function updateDialogTemplate(): string {
  if (!state.updateDialogOpen) return "";
  const result = state.updateResult;
  const auth = state.updateAuth;
  const percent = state.updateContentLength
    ? Math.min(100, Math.round(state.updateDownloaded / state.updateContentLength * 100))
    : null;
  const authLabel = auth?.source === "windows-credential-manager"
    ? "Windows Credential Manager"
    : auth?.source === "github-cli"
      ? "GitHub CLI"
      : auth?.source === "environment"
        ? "環境変数"
        : null;

  return `<div class="update-backdrop" role="presentation">
    <section class="update-dialog" role="dialog" aria-modal="true" aria-labelledby="update-title">
      <header>
        <div><p class="section-kicker">BUKAN UPDATE</p><h2 id="update-title">アプリの更新</h2></div>
        <button class="icon-button close-update-dialog" type="button" ${state.updateInstalling ? "disabled" : ""} aria-label="閉じる">${icons.close}</button>
      </header>
      <div class="update-dialog-body">
        <div class="update-version-line"><span>現在</span><strong>v${escapeHtml(state.appVersion)}</strong>${authLabel ? `<em>${escapeHtml(authLabel)}で認証</em>` : ""}</div>
        ${state.updateChecking ? `<div class="update-working"><span class="reading-spinner"><i></i><i></i><i></i></span><strong>private GitHub Releaseを確認しています</strong><small>最新版の署名とメタデータを取得中です</small></div>` : ""}
        ${state.updateInstalling ? `<div class="update-working">
          <span class="reading-spinner"><i></i><i></i><i></i></span>
          <strong>${percent === null ? "更新をダウンロードしています" : `更新をダウンロードしています · ${percent}%`}</strong>
          <small>署名を検証後、Bukanを終了してインストールします</small>
          <div class="update-progress"><i style="width:${percent ?? 18}%"></i></div>
        </div>` : ""}
        ${!state.updateChecking && !state.updateInstalling && result?.available ? `<div class="update-available">
          <span>UPDATE AVAILABLE</span>
          <h3>v${escapeHtml(result.version ?? "")}</h3>
          <p>${escapeHtml(result.notes?.trim() || "新しいBukanリリースを利用できます。")}</p>
        </div>` : ""}
        ${!state.updateChecking && !state.updateInstalling && result && !result.available ? `<div class="update-current"><span>✓</span><div><strong>最新版です</strong><small>利用可能な更新はありません</small></div></div>` : ""}
        ${state.updateError ? `<p class="update-error">${escapeHtml(state.updateError)}</p>` : ""}
        ${!state.updateChecking && !state.updateInstalling && !auth?.configured ? `<div class="update-auth">
          <h3>GitHubへ接続</h3>
          <p>privateリポジトリのReleaseを取得するため、fine-grained tokenが必要です。対象リポジトリを <strong>Nkzono99/bukan</strong>、権限を <strong>Contents: Read-only</strong> にしてください。</p>
          <label><span>GitHub token</span><input id="update-token" type="password" placeholder="github_pat_…" autocomplete="off" /></label>
          <button class="action-button save-update-token" type="button">安全に保存して確認</button>
          <small>tokenはWindows Credential Managerへ保存され、WorkspaceやGitには書き込みません。gh auth login済みの場合は入力不要です。</small>
        </div>` : ""}
      </div>
      <footer>
        ${auth?.source === "windows-credential-manager" && !state.updateInstalling ? `<button class="secondary-update-action clear-update-token" type="button">保存したtokenを削除</button>` : `<span></span>`}
        <div>
          ${!state.updateInstalling ? `<button class="secondary-update-action check-update" type="button" ${state.updateChecking ? "disabled" : ""}>再確認</button>` : ""}
          ${result?.available && !state.updateInstalling ? `<button class="action-button install-update" type="button">更新して再起動</button>` : ""}
        </div>
      </footer>
    </section>
  </div>`;
}

function loadingTemplate(message = "Google Drive を探しています"): string {
  return `<main class="loading-screen"><div class="loading-mark"><span>B</span><i></i></div><p>${escapeHtml(message)}</p><small>Paperpile のファイルは変更しません</small></main>`;
}

function render(): void {
  applyTheme();
  codexTerminalHost.remove();
  let content = "";
  if (state.loading) content = loadingTemplate(state.loadingLabel);
  else if (!state.library) content = onboardingTemplate();
  else content = workspaceTemplate();
  app.innerHTML = content + updateDialogTemplate();
  attachPdfPreview();
  bindEvents();
  attachCodexTerminal();
}

function bindEvents(): void {
  bindResizeHandles();
  document.querySelectorAll<HTMLElement>(".update-trigger").forEach((element) => {
    element.addEventListener("click", () => void checkForAppUpdate(false));
  });
  document.querySelector<HTMLElement>(".close-update-dialog")?.addEventListener("click", closeUpdateDialog);
  document.querySelector<HTMLElement>(".update-backdrop")?.addEventListener("click", (event) => {
    if (event.target === event.currentTarget) closeUpdateDialog();
  });
  document.querySelector<HTMLElement>(".check-update")?.addEventListener("click", () => void checkForAppUpdate(false));
  document.querySelector<HTMLElement>(".save-update-token")?.addEventListener("click", () => void saveUpdateToken());
  document.querySelector<HTMLElement>(".clear-update-token")?.addEventListener("click", () => void clearUpdateToken());
  document.querySelector<HTMLElement>(".install-update")?.addEventListener("click", () => void installAppUpdate());
  document.querySelectorAll<HTMLElement>("[data-rail-action]").forEach((element) => {
    element.addEventListener("click", () => {
      const action = element.dataset.railAction;
      if (action === "sidebar") {
        state.sidebarCollapsed = !state.sidebarCollapsed;
        render();
      } else if (action === "library") {
        state.mode = "library";
        state.collection = null;
        state.starredOnly = false;
        render();
      } else if (action === "search") {
        focusLibrarySearch();
      } else if (action === "reviews") {
        void openReviews();
      } else if (action === "codex") {
        void toggleCodex();
      } else if (action === "command") {
        openCommandPalette();
      } else if (action === "theme") {
        cycleTheme();
      }
    });
  });
  document.querySelector<HTMLElement>(".sidebar-close")?.addEventListener("click", () => {
    state.sidebarCollapsed = true;
    render();
  });
  document.querySelector<HTMLElement>(".command-trigger")?.addEventListener("click", openCommandPalette);
  document.querySelector<HTMLElement>(".toggle-list")?.addEventListener("click", () => {
    state.listCollapsed = !state.listCollapsed;
    render();
  });
  document.querySelector<HTMLElement>(".open-vscode")?.addEventListener("click", () => void openWorkspaceInVsCode());
  document.querySelector<HTMLElement>(".close-codex")?.addEventListener("click", () => void toggleCodex(false));
  document.querySelector<HTMLElement>(".start-codex")?.addEventListener("click", () => void startCodexTerminal());
  document.querySelector<HTMLElement>(".retry-codex")?.addEventListener("click", () => void refreshCodexStatus(true));
  document.querySelector<HTMLElement>(".stop-codex")?.addEventListener("click", () => void stopCodexTerminal());
  document.querySelector<HTMLElement>(".set-codex-context")?.addEventListener("click", () => void setSelectedPaperAsCodexContext());
  document.querySelector<HTMLElement>(".clear-codex-context")?.addEventListener("click", clearCodexContext);
  document.querySelectorAll<HTMLElement>("[data-view='codex-list'], .codex-list-badge").forEach((element) => {
    element.addEventListener("click", () => {
      state.mode = "library";
      state.codexPaperListVisible = true;
      render();
    });
  });
  document.querySelector<HTMLElement>(".close-codex-paper-list")?.addEventListener("click", () => {
    state.codexPaperListVisible = false;
    render();
  });
  document.querySelector<HTMLElement>(".clear-codex-paper-list")?.addEventListener("click", () => void clearCodexPaperList());
  document.querySelectorAll<HTMLElement>(".persist-codex-paper-list").forEach((element) => {
    element.addEventListener("click", () => {
      void persistCodexPaperList(element.dataset.listDestination ?? "");
    });
  });
  document.querySelectorAll<HTMLElement>("[data-codex-paper-index]").forEach((element) => {
    element.addEventListener("click", () => {
      state.codexPaperListSelection = Number(element.dataset.codexPaperIndex ?? 0);
      render();
    });
  });
  document.querySelectorAll<HTMLElement>("[data-open-library-paper]").forEach((element) => {
    element.addEventListener("click", () => {
      state.selectedId = element.dataset.openLibraryPaper ?? null;
      state.codexPaperListVisible = false;
      state.mode = "library";
      render();
    });
  });
  document.querySelector<HTMLElement>(".command-backdrop")?.addEventListener("click", (event) => {
    if (event.target === event.currentTarget) closeCommandPalette();
  });
  document.querySelectorAll<HTMLElement>("[data-command-action]").forEach((element) => {
    element.addEventListener("click", () => executeCommand(element.dataset.commandAction ?? ""));
  });
  const commandInput = document.querySelector<HTMLInputElement>("#command-input");
  commandInput?.addEventListener("input", () => {
    const query = commandInput.value.trim().toLocaleLowerCase();
    let visibleCount = 0;
    document.querySelectorAll<HTMLElement>(".command-item").forEach((item) => {
      const visible = !query || (item.dataset.commandSearch ?? "").includes(query);
      item.hidden = !visible;
      if (visible) visibleCount += 1;
    });
    document.querySelectorAll(".command-item.keyboard-active").forEach((item) => item.classList.remove("keyboard-active"));
    document.querySelector<HTMLElement>(".command-item:not([hidden])")?.classList.add("keyboard-active");
    document.querySelector<HTMLElement>(".command-empty")?.classList.toggle("visible", visibleCount === 0);
  });
  commandInput?.addEventListener("keydown", (event) => {
    const items = [...document.querySelectorAll<HTMLButtonElement>(".command-item:not([hidden])")];
    if (!items.length) return;
    const currentIndex = items.findIndex((item) => item.classList.contains("keyboard-active"));
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const direction = event.key === "ArrowDown" ? 1 : -1;
      const nextIndex = (Math.max(currentIndex, 0) + direction + items.length) % items.length;
      items.forEach((item) => item.classList.remove("keyboard-active"));
      items[nextIndex]?.classList.add("keyboard-active");
      items[nextIndex]?.scrollIntoView({ block: "nearest" });
    } else if (event.key === "Enter") {
      event.preventDefault();
      (items[Math.max(currentIndex, 0)] as HTMLButtonElement | undefined)?.click();
    }
  });
  document.querySelectorAll<HTMLElement>("[data-library-path]").forEach((element) => {
    element.addEventListener("click", () => {
      state.workspaceRoot = null;
      state.workspaceCollections = null;
      state.workspaceName = null;
      state.managedWorkspace = false;
      state.codexPaperList = null;
      state.codexPaperListVisible = false;
      state.reviewSummaries = [];
      state.activeReview = null;
      state.reviewEditing = false;
      void loadLibrary(element.dataset.libraryPath ?? "");
    });
  });
  document.querySelectorAll<HTMLElement>(".choose-folder, .change-library").forEach((element) => {
    element.addEventListener("click", () => void chooseLibrary());
  });
  document.querySelector<HTMLElement>(".open-workspace")?.addEventListener("click", () => void chooseWorkspace(false));
  document.querySelector<HTMLElement>(".create-workspace")?.addEventListener("click", () => void chooseWorkspace(true));
  document.querySelector<HTMLElement>("[data-view='all']")?.addEventListener("click", () => {
    if (state.mode === "library" && !state.collection && !state.starredOnly) return;
    state.mode = "library";
    state.collection = null;
    state.starredOnly = false;
    state.visibleLimit = 120;
    render();
  });
  document.querySelector<HTMLElement>("[data-view='starred']")?.addEventListener("click", () => {
    if (state.mode === "library" && !state.collection && state.starredOnly) return;
    state.mode = "library";
    state.collection = null;
    state.starredOnly = true;
    state.visibleLimit = 120;
    render();
  });
  document.querySelector<HTMLElement>(".open-review-codex")?.addEventListener("click", () => void updateReviewWithCodex());
  document.querySelectorAll<HTMLElement>("[data-review-id]").forEach((element) => {
    element.addEventListener("click", () => void loadReview(element.dataset.reviewId ?? ""));
  });
  document.querySelector<HTMLElement>(".edit-review")?.addEventListener("click", () => {
    state.reviewEditing = true;
    render();
    window.setTimeout(() => document.querySelector<HTMLTextAreaElement>("#review-article-editor")?.focus(), 0);
  });
  document.querySelector<HTMLElement>(".cancel-review-edit")?.addEventListener("click", () => {
    state.reviewEditing = false;
    render();
  });
  document.querySelector<HTMLElement>(".save-review")?.addEventListener("click", () => void saveReview());
  document.querySelector<HTMLElement>(".review-with-codex")?.addEventListener("click", () => void updateReviewWithCodex());
  document.querySelectorAll<HTMLElement>("[data-collection]").forEach((element) => {
    element.addEventListener("click", () => {
      const collection = element.dataset.collection ?? null;
      if (state.mode === "library" && state.collection === collection && !state.starredOnly) return;
      state.collection = collection;
      state.mode = "library";
      state.starredOnly = false;
      state.visibleLimit = 120;
      render();
    });
  });
  document.querySelector<HTMLElement>(".new-workspace-collection")?.addEventListener("click", () => {
    state.creatingCollection = true;
    render();
    window.setTimeout(() => document.querySelector<HTMLInputElement>("#workspace-collection-path")?.focus(), 0);
  });
  document.querySelector<HTMLElement>(".cancel-workspace-collection")?.addEventListener("click", () => {
    state.creatingCollection = false;
    render();
  });
  document.querySelector<HTMLFormElement>(".workspace-collection-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    const path = document.querySelector<HTMLInputElement>("#workspace-collection-path")?.value.trim() ?? "";
    void createWorkspaceCollection(path);
  });
  document.querySelectorAll<HTMLElement>("[data-workspace-membership]").forEach((element) => {
    element.addEventListener("click", (event) => {
      event.stopPropagation();
      void updateWorkspaceCollectionMembership(
        element.dataset.workspaceMembership ?? "",
        element.dataset.member === "true",
      );
    });
  });
  document.querySelectorAll<HTMLElement>("[data-collection-toggle]").forEach((element) => {
    element.addEventListener("click", () => {
      const path = element.dataset.collectionToggle;
      if (!path) return;
      if (state.collapsedCollections.has(path)) state.collapsedCollections.delete(path);
      else state.collapsedCollections.add(path);
      render();
    });
  });
  document.querySelectorAll<HTMLElement>("[data-paper-id]").forEach((element) => {
    element.addEventListener("click", () => {
      const paperId = element.dataset.paperId ?? null;
      if (state.selectedId === paperId) return;
      state.selectedId = paperId;
      render();
    });
  });
  const searchInput = document.querySelector<HTMLInputElement>("#search-input");
  searchInput?.addEventListener("input", () => {
    state.query = searchInput.value;
    state.visibleLimit = 120;
    const selectionStart = searchInput.selectionStart;
    render();
    const nextInput = document.querySelector<HTMLInputElement>("#search-input");
    nextInput?.focus();
    if (selectionStart !== null) nextInput?.setSelectionRange(selectionStart, selectionStart);
  });
  document.querySelector<HTMLSelectElement>("#sort-select")?.addEventListener("change", (event) => {
    state.sort = (event.currentTarget as HTMLSelectElement).value as SortMode;
    render();
  });
  document.querySelector<HTMLElement>(".load-more")?.addEventListener("click", () => {
    state.visibleLimit += 120;
    render();
  });
  document.querySelector<HTMLElement>(".refresh-library")?.addEventListener("click", () => {
    if (state.library) void loadLibrary(state.library.root, true);
  });
  document.querySelectorAll<HTMLElement>(".open-external").forEach((element) => {
    element.addEventListener("click", () => void runPaperAction("open_paper"));
  });
  document.querySelector<HTMLElement>(".reveal-paper")?.addEventListener("click", () => void runPaperAction("reveal_paper"));
}

function bindResizeHandles(): void {
  document.querySelectorAll<HTMLElement>("[data-resize-pane]").forEach((handle) => {
    const pane = handle.dataset.resizePane;
    const bounds = pane === "sidebar" ? SIDEBAR_WIDTH : PAPER_LIST_WIDTH;
    const storageKey = pane === "sidebar" ? "bukan.sidebarWidth" : "bukan.paperListWidth";
    const updateWidth = (width: number) => {
      const next = Math.round(Math.min(bounds.max, Math.max(bounds.min, width)));
      if (pane === "sidebar") state.sidebarWidth = next;
      else state.paperListWidth = next;
      document.querySelector<HTMLElement>(".workspace")?.style.setProperty(
        pane === "sidebar" ? "--sidebar-width" : "--paper-list-width",
        `${next}px`,
      );
      localStorage.setItem(storageKey, String(next));
    };

    handle.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) return;
      event.preventDefault();
      const startX = event.clientX;
      const startWidth = pane === "sidebar" ? state.sidebarWidth : state.paperListWidth;
      handle.setPointerCapture(event.pointerId);
      document.body.classList.add("resizing-columns");
      const move = (moveEvent: PointerEvent) => updateWidth(startWidth + moveEvent.clientX - startX);
      const finish = () => {
        handle.removeEventListener("pointermove", move);
        document.body.classList.remove("resizing-columns");
        scheduleCodexResize();
      };
      handle.addEventListener("pointermove", move);
      handle.addEventListener("pointerup", finish, { once: true });
      handle.addEventListener("pointercancel", finish, { once: true });
    });

    handle.addEventListener("keydown", (event) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      event.preventDefault();
      const current = pane === "sidebar" ? state.sidebarWidth : state.paperListWidth;
      updateWidth(current + (event.key === "ArrowRight" ? 12 : -12));
      render();
    });

    handle.addEventListener("dblclick", () => {
      updateWidth(bounds.initial);
      render();
    });
  });
}

async function createWorkspaceCollection(path: string): Promise<void> {
  if (!state.workspaceRoot || !path || state.collectionSaving) return;
  state.collectionSaving = true;
  render();
  try {
    state.workspaceCollections = await invoke<WorkspaceCollectionIndex>("create_workspace_collection", {
      workspaceRoot: state.workspaceRoot,
      collectionPath: path,
    });
    state.creatingCollection = false;
    showToast(`Workspace / All / ${displayCollectionPath(path)} を作成しました`);
  } catch (error) {
    showToast(String(error), true);
  } finally {
    state.collectionSaving = false;
    render();
  }
}

async function updateWorkspaceCollectionMembership(path: string, member: boolean): Promise<void> {
  if (!state.workspaceRoot || !state.selectedId || !path || state.collectionSaving) return;
  const paperId = state.selectedId;
  state.collectionSaving = true;
  try {
    state.workspaceCollections = await invoke<WorkspaceCollectionIndex>("set_workspace_collection_membership", {
      workspaceRoot: state.workspaceRoot,
      collectionPath: path,
      paperId,
      member,
    });
    showToast(member
      ? "選択中の文献をWorkspaceコレクションへ追加しました"
      : "選択中の文献をWorkspaceコレクションから外しました");
  } catch (error) {
    showToast(String(error), true);
  } finally {
    state.collectionSaving = false;
    render();
  }
}

function openCommandPalette(): void {
  state.commandOpen = true;
  render();
  window.setTimeout(() => document.querySelector<HTMLInputElement>("#command-input")?.focus(), 0);
}

function closeCommandPalette(): void {
  state.commandOpen = false;
  render();
}

function focusLibrarySearch(): void {
  state.mode = "library";
  state.listCollapsed = false;
  state.commandOpen = false;
  render();
  window.setTimeout(() => document.querySelector<HTMLInputElement>("#search-input")?.focus(), 0);
}

function executeCommand(action: string): void {
  state.commandOpen = false;
  if (action === "search") {
    focusLibrarySearch();
  } else if (action === "all" || action === "starred") {
    state.mode = "library";
    state.collection = null;
    state.starredOnly = action === "starred";
    render();
  } else if (action === "open-vscode") {
    void openWorkspaceInVsCode();
  } else if (action === "reviews") {
    void openReviews();
  } else if (action === "toggle-codex") {
    void toggleCodex();
  } else if (action === "toggle-sidebar") {
    state.sidebarCollapsed = !state.sidebarCollapsed;
    render();
  } else if (action === "toggle-list") {
    state.listCollapsed = !state.listCollapsed;
    render();
  } else if (action.startsWith("theme-")) {
    setTheme(action.slice(6) as ThemeMode);
  } else if (action === "refresh") {
    if (state.library) void loadLibrary(state.library.root, true);
  } else if (action === "check-update") {
    void checkForAppUpdate(false);
  } else {
    render();
  }
}

function ensureCodexTerminal(): void {
  if (codexTerminal) return;
  codexTerminal = new Terminal({
    cursorBlink: true,
    cursorStyle: "bar",
    fontFamily: '"Cascadia Mono", "SFMono-Regular", Consolas, "Liberation Mono", monospace',
    fontSize: 12,
    lineHeight: 1.25,
    letterSpacing: 0,
    scrollback: 12_000,
    allowProposedApi: false,
    theme: terminalTheme(),
  });
  codexFitAddon = new FitAddon();
  codexTerminal.loadAddon(codexFitAddon);
  codexTerminal.open(codexTerminalHost);
  codexTerminal.onData((data) => {
    if (!state.codexStatus?.running) return;
    void invoke("write_codex_terminal", { data }).catch((error) => {
      state.codexError = String(error);
      showToast(state.codexError, true);
    });
  });
  if (codexBacklog) {
    codexTerminal.write(codexBacklog);
    codexBacklog = "";
  }
}

function attachCodexTerminal(): void {
  const mount = document.querySelector<HTMLElement>(".codex-terminal-mount");
  if (!mount || !state.codexOpen || !state.codexStatus?.running) return;
  mount.append(codexTerminalHost);
  ensureCodexTerminal();
  codexResizeObserver?.disconnect();
  codexResizeObserver = new ResizeObserver(() => scheduleCodexResize());
  codexResizeObserver.observe(mount);
  window.requestAnimationFrame(() => {
    fitAndResizeCodex();
    codexTerminal?.focus();
  });
}

function scheduleCodexResize(): void {
  if (codexResizeTimer !== null) window.clearTimeout(codexResizeTimer);
  codexResizeTimer = window.setTimeout(() => {
    codexResizeTimer = null;
    fitAndResizeCodex();
  }, 80);
}

function fitAndResizeCodex(): void {
  if (!state.codexStatus?.running || !codexTerminalHost.isConnected) return;
  try {
    codexFitAddon?.fit();
    if (codexTerminal) {
      void invoke("resize_codex_terminal", {
        cols: codexTerminal.cols,
        rows: codexTerminal.rows,
      }).catch(() => undefined);
    }
  } catch {
    // The pane can be between layouts while the terminal is being moved.
  }
}

async function setupCodexEventListeners(): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) return;
  await listen<CodexTerminalOutput>("codex-terminal-output", ({ payload }) => {
    if (state.codexStatus?.sessionId && payload.sessionId !== state.codexStatus.sessionId) return;
    if (codexTerminal) {
      codexTerminal.write(payload.data);
    } else {
      codexBacklog = `${codexBacklog}${payload.data}`.slice(-4_000_000);
    }
  });
  await listen<CodexTerminalExit>("codex-terminal-exit", ({ payload }) => {
    if (state.codexStatus?.sessionId !== payload.sessionId) return;
    state.codexStatus = {
      ...state.codexStatus,
      running: false,
      sessionId: null,
      workspaceRoot: null,
    };
    render();
  });
}

async function setupUpdateIntegration(): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window)) {
    if (isDemoMode && new URLSearchParams(window.location.search).get("update") === "available") {
      state.updateAuth = {
        configured: true,
        source: "github-cli",
        credentialStorageAvailable: true,
      };
      state.updateResult = {
        currentVersion: state.appVersion,
        available: true,
        version: "0.2.0",
        notes: "Paperpile差分監視の改善\nCodex文献リスト連携の更新\nUpdaterによる安全な配布",
        publishedAt: new Date().toISOString(),
        authSource: "github-cli",
      };
      state.updateDialogOpen = true;
    }
    return;
  }
  try {
    state.appVersion = await getVersion();
    state.updateAuth = await invoke<UpdateAuthStatus>("update_auth_status");
    await listen<AppUpdateProgress>("app-update-progress", ({ payload }) => {
      if (payload.event === "started") {
        state.updateDownloaded = 0;
        state.updateContentLength = payload.data?.contentLength ?? null;
      } else if (payload.event === "progress") {
        state.updateDownloaded = payload.data?.downloaded ?? state.updateDownloaded;
        state.updateContentLength = payload.data?.contentLength ?? state.updateContentLength;
      } else if (payload.event === "finished") {
        state.updateDownloaded = state.updateContentLength ?? state.updateDownloaded;
      }
      render();
    });
    window.setTimeout(() => void checkForAppUpdate(true), 1800);
  } catch (error) {
    console.warn("Could not initialize app updates", error);
  }
}

function closeUpdateDialog(): void {
  if (state.updateInstalling) return;
  state.updateDialogOpen = false;
  render();
}

async function checkForAppUpdate(quiet: boolean): Promise<void> {
  if (!("__TAURI_INTERNALS__" in window) || state.updateChecking || state.updateInstalling) return;
  state.updateError = null;
  if (!quiet) state.updateDialogOpen = true;
  try {
    state.updateAuth = await invoke<UpdateAuthStatus>("update_auth_status");
    if (!state.updateAuth.configured) {
      if (!quiet) {
        state.updateError = state.updateAuth.detail ?? "private Releaseへ接続するGitHub認証を設定してください。";
        render();
      }
      return;
    }
    state.updateChecking = true;
    render();
    state.updateResult = await invoke<AppUpdateCheckResult>("check_app_update");
    if (state.updateResult.available) {
      state.updateDialogOpen = true;
      if (quiet) showToast(`Bukan v${state.updateResult.version} を利用できます`);
    } else if (!quiet) {
      showToast("Bukanは最新版です");
    }
  } catch (error) {
    state.updateError = String(error);
    if (!quiet) state.updateDialogOpen = true;
  } finally {
    state.updateChecking = false;
    render();
  }
}

async function saveUpdateToken(): Promise<void> {
  const token = document.querySelector<HTMLInputElement>("#update-token")?.value.trim() ?? "";
  if (!token) {
    state.updateError = "GitHub tokenを入力してください。";
    render();
    return;
  }
  state.updateChecking = true;
  state.updateError = null;
  render();
  try {
    state.updateAuth = await invoke<UpdateAuthStatus>("save_update_github_token", { token });
    state.updateChecking = false;
    await checkForAppUpdate(false);
  } catch (error) {
    state.updateChecking = false;
    state.updateError = String(error);
    render();
  }
}

async function clearUpdateToken(): Promise<void> {
  try {
    state.updateAuth = await invoke<UpdateAuthStatus>("clear_update_github_token");
    state.updateResult = null;
    state.updateError = state.updateAuth.configured
      ? `保存済みtokenを削除しました。${state.updateAuth.source === "github-cli" ? "引き続きGitHub CLIの認証を利用します。" : ""}`
      : null;
    render();
  } catch (error) {
    state.updateError = String(error);
    render();
  }
}

async function installAppUpdate(): Promise<void> {
  if (!state.updateResult?.available || state.updateInstalling) return;
  state.updateInstalling = true;
  state.updateError = null;
  state.updateDownloaded = 0;
  state.updateContentLength = null;
  render();
  try {
    await invoke("install_app_update");
  } catch (error) {
    state.updateInstalling = false;
    state.updateError = String(error);
    render();
  }
}

async function openReviews(): Promise<void> {
  if (!state.workspaceRoot) {
    showToast("レビューの作業領域を準備できませんでした。ライブラリを開き直してください。", true);
    return;
  }
  state.mode = "reviews";
  state.reviewEditing = false;
  render();
  await refreshReviews(true);
}

async function refreshReviews(selectFirst = false): Promise<void> {
  if (!state.workspaceRoot || !("__TAURI_INTERNALS__" in window)) return;
  try {
    const summaries = await invoke<ReviewSummary[]>("list_research_reviews", {
      workspaceRoot: state.workspaceRoot,
    });
    const activeSummary = summaries.find((review) => review.id === state.activeReview?.id);
    const activeChanged = activeSummary && activeSummary.updatedAt !== state.activeReview?.updatedAt;
    state.reviewSummaries = summaries;
    if (activeChanged) {
      await loadReview(activeSummary.id, false);
      return;
    }
    if (selectFirst && !state.activeReview && summaries[0]) {
      await loadReview(summaries[0].id, false);
      return;
    }
    if (state.mode === "reviews") render();
  } catch (error) {
    console.warn("Could not refresh research reviews", error);
  }
}

async function loadReview(reviewId: string, showLoading = true): Promise<void> {
  if (!state.workspaceRoot || !reviewId) return;
  state.mode = "reviews";
  state.reviewEditing = false;
  if (showLoading) {
    state.reviewLoading = true;
    render();
  }
  try {
    state.activeReview = await invoke<ReviewDocument>("get_research_review", {
      workspaceRoot: state.workspaceRoot,
      reviewId,
    });
    await invoke("set_current_research_review", {
      workspaceRoot: state.workspaceRoot,
      reviewId,
    });
  } catch (error) {
    showToast(String(error), true);
  } finally {
    state.reviewLoading = false;
    render();
  }
}

async function saveReview(): Promise<void> {
  if (!state.workspaceRoot || !state.activeReview || state.reviewSaving) return;
  const article = document.querySelector<HTMLTextAreaElement>("#review-article-editor")?.value;
  if (article === undefined) return;
  state.reviewSaving = true;
  render();
  try {
    state.activeReview = await invoke<ReviewDocument>("save_research_review", {
      workspaceRoot: state.workspaceRoot,
      reviewId: state.activeReview.id,
      article,
    });
    state.reviewEditing = false;
    await refreshReviews();
    showToast(`改訂 ${state.activeReview.revision} を保存しました`);
  } catch (error) {
    showToast(String(error), true);
  } finally {
    state.reviewSaving = false;
    render();
  }
}

async function updateReviewWithCodex(): Promise<void> {
  if (!state.workspaceRoot) return;
  try {
    if (state.activeReview) {
      await invoke("set_current_research_review", {
        workspaceRoot: state.workspaceRoot,
        reviewId: state.activeReview.id,
      });
      showToast(`Codexへ「${state.activeReview.title}」を現在のレビューとして設定しました`);
    } else {
      showToast("Codexへテーマを伝えると、MCP経由で継続レビューを作成できます");
    }
    await toggleCodex(true);
  } catch (error) {
    showToast(String(error), true);
  }
}

async function refreshCodexStatus(showErrors = false): Promise<CodexRuntimeStatus | null> {
  if (!("__TAURI_INTERNALS__" in window)) {
    state.codexStatus = {
      available: false,
      version: null,
      command: null,
      running: false,
      sessionId: null,
      workspaceRoot: null,
    };
    render();
    return state.codexStatus;
  }
  try {
    state.codexError = null;
    state.codexStatus = await invoke<CodexRuntimeStatus>("codex_runtime_status");
    render();
    return state.codexStatus;
  } catch (error) {
    state.codexError = String(error);
    if (showErrors) showToast(state.codexError, true);
    render();
    return null;
  }
}

async function toggleCodex(force?: boolean): Promise<void> {
  const next = force ?? !state.codexOpen;
  if (next && !state.workspaceRoot) {
    showToast("Codexを使うには、先にBukanワークスペースを開いてください", true);
    return;
  }
  state.codexOpen = next;
  if (!next) {
    codexResizeObserver?.disconnect();
    render();
    return;
  }
  state.mode = "library";
  render();
  const status = await refreshCodexStatus();
  if (status?.available && !status.running) await startCodexTerminal();
}

async function startCodexTerminal(): Promise<void> {
  if (!state.workspaceRoot || state.codexStarting) return;
  state.codexOpen = true;
  state.codexStarting = true;
  state.codexError = null;
  codexBacklog = "";
  codexTerminal?.reset();
  render();
  try {
    state.codexStatus = await invoke<CodexRuntimeStatus>("start_codex_terminal", {
      workspaceRoot: state.workspaceRoot,
      cols: codexTerminal?.cols ?? 110,
      rows: codexTerminal?.rows ?? 32,
    });
  } catch (error) {
    state.codexError = String(error);
    state.codexStatus = {
      available: !state.codexError.includes("見つかりません"),
      version: null,
      command: null,
      running: false,
      sessionId: null,
      workspaceRoot: null,
    };
    showToast(state.codexError, true);
  } finally {
    state.codexStarting = false;
    render();
  }
}

async function stopCodexTerminal(): Promise<void> {
  try {
    await invoke("stop_codex_terminal");
    if (state.codexStatus) {
      state.codexStatus.running = false;
      state.codexStatus.sessionId = null;
      state.codexStatus.workspaceRoot = null;
    }
    render();
  } catch (error) {
    showToast(String(error), true);
  }
}

let codexPaperListPolling = false;

async function refreshCodexPaperList(): Promise<void> {
  if (!state.workspaceRoot || codexPaperListPolling || !("__TAURI_INTERNALS__" in window)) return;
  codexPaperListPolling = true;
  try {
    const next = await invoke<PresentedPaperList | null>("get_codex_paper_list", {
      workspaceRoot: state.workspaceRoot,
    });
    const changed = next?.updatedAt !== state.codexPaperList?.updatedAt;
    if (!changed) return;
    const hadList = Boolean(state.codexPaperList);
    state.codexPaperList = next;
    state.codexPaperListSelection = 0;
    state.codexPaperListVisible = Boolean(next);
    if (next) showToast(`${next.title} · ${next.papers.length}件をCodexから受け取りました`);
    else if (hadList) state.codexPaperListVisible = false;
    render();
  } catch (error) {
    console.warn("Could not refresh Codex paper list", error);
  } finally {
    codexPaperListPolling = false;
  }
}

async function clearCodexPaperList(): Promise<void> {
  if (!state.workspaceRoot) return;
  try {
    await invoke("clear_codex_paper_list", { workspaceRoot: state.workspaceRoot });
    state.codexPaperList = null;
    state.codexPaperListVisible = false;
    render();
  } catch (error) {
    showToast(String(error), true);
  }
}

async function persistCodexPaperList(destination: string): Promise<void> {
  if (!state.workspaceRoot || !state.codexPaperList) return;
  try {
    const path = await invoke<string>("persist_codex_paper_list", {
      workspaceRoot: state.workspaceRoot,
      destination,
    });
    showToast(`${destination === "reports" ? "レポート" : "候補リスト"}へ保存しました · ${path}`);
  } catch (error) {
    showToast(String(error), true);
  }
}

async function setSelectedPaperAsCodexContext(): Promise<void> {
  const paper = state.library?.papers.find((candidate) => candidate.id === state.selectedId);
  if (!paper || !state.workspaceRoot) return;
  try {
    await invoke<string>("set_codex_paper_context", {
      workspaceRoot: state.workspaceRoot,
      paperPath: paper.path,
      paperId: paper.id,
      title: paper.title,
      authors: paper.authors,
      year: paper.year,
      collections: paper.collections,
    });
    state.codexContextPaperId = paper.id;
    showToast("現在の論文をCodexコンテキストに設定しました");
    if (!state.codexOpen) await toggleCodex(true);
    else render();
  } catch (error) {
    showToast(String(error), true);
  }
}

function clearCodexContext(): void {
  if (!state.workspaceRoot) return;
  void invoke("clear_codex_paper_context", { workspaceRoot: state.workspaceRoot })
    .then(() => {
      state.codexContextPaperId = null;
      render();
    })
    .catch((error) => showToast(String(error), true));
}

async function openWorkspaceInVsCode(): Promise<void> {
  if (!state.workspaceRoot) {
    showToast("先にBukanワークスペースを開いてください", true);
    return;
  }
  try {
    await invoke("open_workspace_in_vscode", { workspaceRoot: state.workspaceRoot });
    showToast("VS Codeでワークスペースを開きました");
  } catch (error) {
    showToast(`${String(error)} — VS Codeがインストールされ、vscode:// URLが有効か確認してください`, true);
  }
}

async function runPaperAction(command: "open_paper" | "reveal_paper"): Promise<void> {
  const paper = state.library?.papers.find((candidate) => candidate.id === state.selectedId);
  if (!paper) return;
  try {
    await invoke(command, { path: paper.path });
  } catch (error) {
    showToast(String(error), true);
  }
}

async function chooseLibrary(): Promise<void> {
  try {
    const selected = await open({ directory: true, multiple: false, title: "Paperpile フォルダを選択" });
    if (selected) {
      if (state.codexStatus?.running) await invoke("stop_codex_terminal");
      state.workspaceRoot = null;
      state.workspaceName = null;
      state.managedWorkspace = false;
      state.codexOpen = false;
      state.codexStatus = null;
      state.codexContextPaperId = null;
      state.codexPaperList = null;
      state.codexPaperListVisible = false;
      state.reviewSummaries = [];
      state.activeReview = null;
      state.reviewEditing = false;
      state.libraryChangeToken = null;
      state.libraryLastCheckedAt = null;
      localStorage.removeItem("bukan.workspaceRoot");
      await loadLibrary(selected);
    }
  } catch (error) {
    state.error = String(error);
    render();
  }
}

async function chooseWorkspace(create: boolean): Promise<void> {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: create ? "ワークスペースを作成するフォルダ" : "Bukan ワークスペースを選択",
    });
    if (!selected) return;
    state.loadingLabel = create ? "ワークスペースを初期化しています" : "ワークスペースを開いています";
    state.loading = true;
    render();
    const descriptor = create
      ? await invoke<WorkspaceDescriptor>("initialize_workspace", { root: selected, name: null, paperpilePath: "auto" })
      : await invoke<WorkspaceDescriptor>("open_workspace", { root: selected });
    await activateWorkspace(descriptor);
  } catch (error) {
    state.loading = false;
    state.error = String(error);
    render();
  }
}

async function activateWorkspace(descriptor: WorkspaceDescriptor): Promise<void> {
  if (!descriptor.paperpileRoot) {
    throw new Error("マウント済みの Paperpile が見つかりません。bukan.toml の paperpile.path を確認してください。");
  }
  if (state.workspaceRoot && state.workspaceRoot !== descriptor.root && state.codexStatus?.running) {
    await invoke("stop_codex_terminal");
  }
  state.workspaceRoot = descriptor.root;
  state.workspaceCollections = null;
  state.workspaceName = descriptor.name;
  state.managedWorkspace = false;
  state.codexStatus = null;
  state.codexContextPaperId = null;
  state.codexPaperList = null;
  state.codexPaperListVisible = false;
  state.reviewSummaries = [];
  state.activeReview = null;
  state.reviewEditing = false;
  state.libraryChangeToken = null;
  state.libraryLastCheckedAt = null;
  localStorage.setItem("bukan.workspaceRoot", descriptor.root);
  await loadLibrary(descriptor.paperpileRoot);
}

async function loadLibrary(root: string, quiet = false): Promise<void> {
  if (!root) return;
  const previous = state.library?.root === root ? state.library : null;
  state.error = null;
  if (quiet) {
    state.scanning = true;
    render();
  } else {
    state.loadingLabel = "Paperpile ライブラリを読み取っています";
    state.loading = true;
    render();
  }
  try {
    const index = await invoke<LibraryIndex>("scan_library", { root });
    if (!state.workspaceRoot) {
      try {
        const descriptor = await invoke<WorkspaceDescriptor>("ensure_managed_workspace", {
          paperpileRoot: index.root,
        });
        state.workspaceRoot = descriptor.root;
        state.workspaceName = null;
        state.managedWorkspace = true;
      } catch (error) {
        console.warn("Could not prepare the managed Codex workspace", error);
        showToast(`Codexの作業領域を準備できませんでした: ${String(error)}`, true);
      }
    }
    state.library = index;
    if (state.workspaceRoot) {
      try {
        state.workspaceCollections = await invoke<WorkspaceCollectionIndex>("load_workspace_collections", {
          workspaceRoot: state.workspaceRoot,
          seeds: index.papers.map((paper) => ({
            paperId: paper.id,
            collections: paper.collections,
          })),
        });
      } catch (error) {
        console.warn("Could not load Workspace collections", error);
        showToast(`Workspaceコレクションを読み取れませんでした: ${String(error)}`, true);
      }
    }
    await refreshReviews();
    state.selectedId = state.selectedId && index.papers.some((paper) => paper.id === state.selectedId) ? state.selectedId : null;
    localStorage.setItem("bukan.paperpileRoot", index.root);
    if (quiet) {
      const previousIds = new Set(previous?.papers.map((paper) => paper.id) ?? []);
      const nextIds = new Set(index.papers.map((paper) => paper.id));
      const added = index.papers.filter((paper) => !previousIds.has(paper.id)).length;
      const removed = (previous?.papers ?? []).filter((paper) => !nextIds.has(paper.id)).length;
      const changes = [
        added ? `追加 ${added}` : "",
        removed ? `削除 ${removed}` : "",
      ].filter(Boolean).join(" / ");
      showToast(changes ? `Paperpile更新 · ${changes} · 全${index.stats.paperCount}件` : `${index.stats.paperCount}件を再読込しました`);
    }
  } catch (error) {
    state.error = String(error);
    if (!quiet) state.library = null;
  } finally {
    state.loading = false;
    state.scanning = false;
    render();
    window.setTimeout(() => void refreshLibraryChangeToken(), 0);
  }
}

let libraryChangePolling = false;

async function refreshLibraryChangeToken(): Promise<void> {
  if (
    !("__TAURI_INTERNALS__" in window)
    || !state.library
    || state.scanning
    || state.loading
    || libraryChangePolling
  ) return;

  libraryChangePolling = true;
  try {
    const root = state.library.root;
    const result = await invoke<LibraryChangeToken>("library_change_token", { root });
    const previousToken = state.libraryChangeToken;
    state.libraryLastCheckedAt = result.checkedAt;
    state.libraryChangeToken = result.token;
    if (previousToken && previousToken !== result.token && state.library?.root === root) {
      await loadLibrary(root, true);
      const refreshed = await invoke<LibraryChangeToken>("library_change_token", { root });
      state.libraryChangeToken = refreshed.token;
      state.libraryLastCheckedAt = refreshed.checkedAt;
    } else if (!previousToken) {
      render();
    }
  } catch (error) {
    console.warn("Could not monitor Paperpile changes", error);
  } finally {
    libraryChangePolling = false;
  }
}

function showToast(message: string, isError = false): void {
  document.querySelector(".toast")?.remove();
  const toast = document.createElement("div");
  toast.className = `toast ${isError ? "toast-error" : ""}`;
  toast.textContent = message;
  document.body.append(toast);
  window.setTimeout(() => toast.classList.add("visible"), 20);
  window.setTimeout(() => {
    toast.classList.remove("visible");
    window.setTimeout(() => toast.remove(), 250);
  }, 3200);
}

function demoLibrary(): LibraryIndex {
  const demoPapers: PaperRecord[] = [
    {
      id: "demo-1", title: "Water on the Moon: A Review of Current Observations and Future Prospects",
      authors: "Li et al.", year: 2025, collections: ["My Papers / 月の水資源 / 観測", "My Papers / お気に入り"], path: "C:\\demo\\moon-water.pdf",
      relativePath: "月の水資源\\Li 2025 - Water on the Moon.pdf", fileName: "Li 2025 - Water on the Moon.pdf",
      sizeBytes: 4_820_000, modifiedAt: Date.now() - 86_400_000, starred: true,
    },
    {
      id: "demo-2", title: "Electrostatic Charging and Dust Transport on the Lunar Surface",
      authors: "Sato & Nakamura", year: 2024, collections: ["My Papers / 月面環境 / 月面帯電"], path: "C:\\demo\\lunar-dust.pdf",
      relativePath: "月面帯電\\Sato 2024 - Electrostatic Charging.pdf", fileName: "Sato 2024 - Electrostatic Charging.pdf",
      sizeBytes: 2_640_000, modifiedAt: Date.now() - 172_800_000, starred: false,
    },
    {
      id: "demo-3", title: "かぐや観測データによる月面プラズマ環境の統計解析",
      authors: "山田 太郎ほか", year: 2023, collections: ["My Papers / ミッション / かぐや観測解析"], path: "C:\\demo\\kaguya.pdf",
      relativePath: "かぐや観測解析\\山田 2023 - 月面プラズマ環境.pdf", fileName: "山田 2023 - 月面プラズマ環境.pdf",
      sizeBytes: 8_120_000, modifiedAt: Date.now() - 259_200_000, starred: false,
    },
    {
      id: "demo-4", title: "Semi-implicit Particle-in-Cell Methods for Space Plasma Simulation",
      authors: "Chen et al.", year: 2022, collections: ["My Papers / Methods / Numerical", "My Papers / ツール系"], path: "C:\\demo\\pic-methods.pdf",
      relativePath: "Methods\\Chen 2022 - Semi-implicit PIC.pdf", fileName: "Chen 2022 - Semi-implicit PIC.pdf",
      sizeBytes: 12_450_000, modifiedAt: Date.now() - 345_600_000, starred: false,
    },
  ];
  const collections = [...new Set(demoPapers.flatMap((paper) => paper.collections))].sort();
  return {
    root: "G:\\マイドライブ\\Paperpile",
    papers: demoPapers,
    collections,
    stats: {
      paperCount: demoPapers.length,
      collectionCount: collections.length,
      starredCount: demoPapers.filter((paper) => paper.starred).length,
      totalBytes: demoPapers.reduce((total, paper) => total + paper.sizeBytes, 0),
    },
    warnings: [],
  };
}

function demoReview(): ReviewDocument {
  const now = Math.floor(Date.now() / 1000);
  return {
    id: "demo-lunar-dust-review",
    theme: "月面ダスト輸送に対する静電場の影響",
    title: "月面ダストの静電輸送：観測・実験・数値モデルの統合レビュー",
    revision: 4,
    createdAt: now - 2_592_000,
    updatedAt: now - 3_600,
    citations: [
      {
        paperId: "demo-1",
        title: "Electrostatic Dust Transport on the Lunar Surface",
        authors: "Sato, K. & Miller, J.",
        year: 2024,
        locator: "pp. 12–14, Fig. 3",
        note: "終端速度の推定",
      },
      {
        paperId: "demo-2",
        title: "Lunar Dust Dynamics: A Comprehensive Review",
        authors: "García et al.",
        year: 2023,
        locator: "§4.2",
        note: "観測制約の整理",
      },
    ],
    figures: [],
    article: `# 月面ダストの静電輸送

> Theme: 月面ダスト輸送に対する静電場の影響

## Scope

月面表層で帯電したダストが浮遊・移動する過程について、観測、実験、数値モデルの整合性を継続的に検討する。

## Evidence synthesis

光電子放出による表面電位は局所時刻とプラズマ条件に強く依存し、粒径ごとの浮遊可能性を変化させる [@demo-1, pp. 12–14]。

既存レビューが示す地平線発光の解釈には未確定要素が残り、直接観測と輸送モデルを分けて評価する必要がある [@demo-2, §4.2]。

## Open questions

- 微小スケールの電場構造を全球モデルへどう接続するか
- 観測されたダストフラックスを一意に説明できる粒径分布

## References

引用情報は右側のエビデンス欄に論文IDと根拠位置付きで保持する。`,
    directory: "C:\\demo\\workspace\\reports\\reviews\\demo-lunar-dust-review",
  };
}

async function initialize(): Promise<void> {
  applyTheme();
  await setupCodexEventListeners();
  await setupUpdateIntegration();
  if (isDemoMode) {
    state.library = demoLibrary();
    state.workspaceCollections = {
      version: 1,
      initializedFrom: "paperpile",
      collections: [
        { path: "月の水資源 / 観測", paperIds: ["demo-1"] },
        { path: "月面環境 / 月面帯電", paperIds: ["demo-2"] },
        { path: "ミッション / かぐや観測解析", paperIds: ["demo-3"] },
        { path: "Methods / Numerical", paperIds: ["demo-4"] },
      ],
    };
    state.workspaceRoot = "C:\\demo\\workspace";
    state.workspaceName = "Bukan";
    if (new URLSearchParams(window.location.search).has("reviews")) {
      const review = demoReview();
      state.mode = "reviews";
      state.activeReview = review;
      state.reviewSummaries = [{
        id: review.id,
        theme: review.theme,
        title: review.title,
        revision: review.revision,
        createdAt: review.createdAt,
        updatedAt: review.updatedAt,
        citationCount: review.citations.length,
        figureCount: review.figures.length,
      }];
    }
    state.loading = false;
    render();
    return;
  }
  render();
  try {
    const savedWorkspace = localStorage.getItem("bukan.workspaceRoot");
    if (savedWorkspace) {
      try {
        const descriptor = await invoke<WorkspaceDescriptor>("open_workspace", { root: savedWorkspace });
        await activateWorkspace(descriptor);
        return;
      } catch {
        localStorage.removeItem("bukan.workspaceRoot");
      }
    }
    state.locations = await invoke<LibraryLocation[]>("detect_libraries");
    const savedRoot = localStorage.getItem("bukan.paperpileRoot");
    const preferred = savedRoot ?? (state.locations.length === 1 ? state.locations[0]?.path : null);
    if (preferred) {
      await loadLibrary(preferred);
      return;
    }
  } catch (error) {
    const message = String(error);
    state.error = message.includes("reading 'invoke'")
      ? "文献の読み込みは Tauri デスクトップアプリから利用できます。"
      : message;
  }
  state.loading = false;
  render();
}

window.addEventListener("keydown", (event) => {
  const target = event.target as HTMLElement | null;
  const isTyping = target?.matches("input, textarea, select, [contenteditable='true']") ?? false;
  if (target?.closest(".xterm")) return;
  if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLocaleLowerCase() === "k") {
    event.preventDefault();
    if (state.commandOpen) closeCommandPalette();
    else openCommandPalette();
  } else if (event.key === "Escape" && state.updateDialogOpen && !state.updateInstalling) {
    event.preventDefault();
    closeUpdateDialog();
  } else if (event.key === "Escape" && state.commandOpen) {
    event.preventDefault();
    closeCommandPalette();
  } else if (event.key === "/" && !isTyping && state.library) {
    event.preventDefault();
    focusLibrarySearch();
  } else if ((event.ctrlKey || event.metaKey) && event.key.toLocaleLowerCase() === "b" && state.library) {
    event.preventDefault();
    state.sidebarCollapsed = !state.sidebarCollapsed;
    render();
  } else if ((event.ctrlKey || event.metaKey) && event.key.toLocaleLowerCase() === "j" && state.library) {
    event.preventDefault();
    void toggleCodex();
  } else if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.key.toLocaleLowerCase() === "k" && state.selectedId) {
    event.preventDefault();
    void setSelectedPaperAsCodexContext();
  }
});

window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  if (state.theme === "system") applyTheme();
});

window.setInterval(() => void refreshCodexPaperList(), 1200);
window.setInterval(() => void refreshReviews(), 2500);
window.setInterval(() => void refreshLibraryChangeToken(), 30000);

void initialize();
