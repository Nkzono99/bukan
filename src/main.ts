import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import "./styles.css";

interface LibraryLocation {
  path: string;
  drive: string;
  displayName: string;
}

interface PaperRecord {
  id: string;
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

interface WorkspaceDescriptor {
  root: string;
  name: string;
  version: number;
  paperpileRoot: string | null;
  paperpileMode: string;
}

interface OrganizationAssignment {
  paperId: string;
  title: string;
  currentCollections: string[];
  suggestedFolders: string[];
  suggestedLabels: string[];
  evidence: string[];
  reviewRequired: boolean;
}

interface OrganizationPlan {
  taxonomyVersion: number;
  folderRoot: string;
  paperCount: number;
  classifiedCount: number;
  reviewRequiredCount: number;
  assignments: OrganizationAssignment[];
}

interface CollectionNode {
  name: string;
  path: string;
  paperIds: Set<string>;
  children: Map<string, CollectionNode>;
}

type SortMode = "recent" | "title" | "year";
type MainMode = "library" | "organize";

const app = document.querySelector<HTMLDivElement>("#app") as HTMLDivElement;
if (!app) throw new Error("App mount point was not found");
const isDemoMode = import.meta.env.DEV && new URLSearchParams(window.location.search).has("demo");

const state: {
  loading: boolean;
  loadingLabel: string;
  scanning: boolean;
  locations: LibraryLocation[];
  library: LibraryIndex | null;
  workspaceRoot: string | null;
  workspaceName: string | null;
  query: string;
  collection: string | null;
  starredOnly: boolean;
  sort: SortMode;
  mode: MainMode;
  selectedId: string | null;
  visibleLimit: number;
  error: string | null;
  organizationPlan: OrganizationPlan | null;
  organizationLoading: boolean;
  collapsedCollections: Set<string>;
} = {
  loading: true,
  loadingLabel: "Google Drive のマウントを探しています",
  scanning: false,
  locations: [],
  library: null,
  workspaceRoot: null,
  workspaceName: null,
  query: "",
  collection: null,
  starredOnly: false,
  sort: "recent",
  mode: "library",
  selectedId: null,
  visibleLimit: 120,
  error: null,
  organizationPlan: null,
  organizationLoading: false,
  collapsedCollections: new Set<string>(),
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
};

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

function buildCollectionTree(library: LibraryIndex): CollectionNode[] {
  const roots = new Map<string, CollectionNode>();
  for (const paper of library.papers) {
    for (const originalCollection of paper.collections) {
      const displayPath = displayCollectionPath(originalCollection);
      if (!displayPath) continue;
      const parts = displayPath.split(" / ");
      let siblings = roots;
      let accumulated = "";
      for (const part of parts) {
        accumulated = accumulated ? `${accumulated} / ${part}` : part;
        let node = siblings.get(part);
        if (!node) {
          node = { name: part, path: accumulated, paperIds: new Set<string>(), children: new Map<string, CollectionNode>() };
          siblings.set(part, node);
        }
        node.paperIds.add(paper.id);
        siblings = node.children;
      }
    }
  }
  const sortNodes = (nodes: Iterable<CollectionNode>): CollectionNode[] => [...nodes].sort((left, right) => left.name.localeCompare(right.name, "ja"));
  const finalize = (nodes: Iterable<CollectionNode>): CollectionNode[] => sortNodes(nodes).map((node) => {
    const children = finalize(node.children.values());
    node.children = new Map(children.map((child) => [child.name, child]));
    return node;
  });
  return finalize(roots.values());
}

function collectionTreeItemTemplate(node: CollectionNode, depth = 0): string {
  const children = [...node.children.values()];
  const hasChildren = children.length > 0;
  const collapsed = state.collapsedCollections.has(node.path);
  return `<div class="collection-node ${collapsed ? "collapsed" : ""}">
    <div class="collection-node-row ${state.collection === node.path ? "active" : ""}" style="--tree-depth:${depth}">
      ${hasChildren
        ? `<button class="collection-toggle" data-collection-toggle="${escapeHtml(node.path)}" aria-label="${collapsed ? "展開" : "折りたたむ"}" aria-expanded="${!collapsed}">${icons.chevron}</button>`
        : `<span class="collection-toggle-spacer"></span>`}
      <button class="collection-select" data-collection="${escapeHtml(node.path)}" title="${escapeHtml(node.path)}">
        ${icons.folder}<i>${escapeHtml(node.name)}</i><em>${node.paperIds.size}</em>
      </button>
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
    if (state.collection && !paper.collections.some((collection) => {
      const displayPath = displayCollectionPath(collection);
      return displayPath === state.collection || displayPath.startsWith(`${state.collection} / `);
    })) return false;
    if (!terms.length) return true;
    const haystack = [
      paper.title,
      paper.authors ?? "",
      paper.year?.toString() ?? "",
      paper.fileName,
      ...paper.collections,
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

  return `<aside class="sidebar">
    <div class="brand"><span class="brand-mark">B</span><span><strong>BUKAN</strong><small>${escapeHtml(state.workspaceName ?? "文献プレビュー")}</small></span></div>
    <nav class="primary-nav" aria-label="ライブラリ">
      <p class="nav-label">ライブラリ</p>
      <button class="nav-item ${!state.collection && !state.starredOnly ? "active" : ""}" data-view="all">
        <span>${icons.library}すべての文献</span><em>${library.stats.paperCount}</em>
      </button>
      <button class="nav-item ${state.starredOnly ? "active" : ""}" data-view="starred">
        <span>${icons.star}スター付き</span><em>${library.stats.starredCount}</em>
      </button>
      <button class="nav-item organize-nav ${state.mode === "organize" ? "active" : ""}" data-view="organize" title="${state.workspaceRoot ? "Bukan分類候補を確認" : "ワークスペースを開くと利用できます"}">
        <span>${icons.folder}Bukan 整理</span><em>${state.workspaceRoot ? "→" : "—"}</em>
      </button>
      <p class="nav-label collections-label">コレクション <em>${visibleCollectionCount}</em></p>
      <div class="collection-list">
        ${collectionTree.map((node) => collectionTreeItemTemplate(node)).join("")}
      </div>
    </nav>
    <div class="sidebar-footer">
      <div class="drive-status"><span>${state.workspaceRoot ? icons.folder : icons.drive}</span><div><strong>${escapeHtml(state.workspaceName ?? "Google Drive")}</strong><small>${escapeHtml(state.workspaceRoot ?? library.root)}</small></div><i></i></div>
      <button class="change-library" type="button">ライブラリを変更</button>
    </div>
  </aside>`;
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

  const isDesktopRuntime = "__TAURI_INTERNALS__" in window;
  const pdfContent = isDesktopRuntime
    ? `<object id="pdf-object" class="pdf-object" data="${escapeHtml(convertFileSrc(paper.path))}" type="application/pdf">
        <div class="pdf-fallback"><p>PDF プレビューを表示できませんでした。</p><button class="action-button open-external">${icons.external}既定のアプリで開く</button></div>
      </object>
      <div id="pdf-loading" class="pdf-loading" role="status">
        <span class="reading-spinner"><i></i><i></i><i></i></span>
        <strong>PDF を読み込んでいます</strong>
        <small>Google Drive 上のファイルを準備しています</small>
      </div>`
    : `<div class="demo-pdf-page" aria-label="PDF preview placeholder">
        <small>BUKAN · READING PREVIEW</small><h3>${escapeHtml(paper.title)}</h3>
        <p>${escapeHtml(paper.authors ?? "")}${paper.year ? ` · ${paper.year}` : ""}</p>
        <i></i><i></i><i></i><i></i><i></i><i></i><i></i><i></i>
      </div>`;
  return `<section class="viewer">
    <header class="viewer-header">
      <div class="viewer-title-wrap">
        <p class="section-kicker">NOW READING</p>
        <h2 title="${escapeHtml(paper.title)}">${escapeHtml(paper.title)}</h2>
        <p>${escapeHtml([paper.authors, paper.year?.toString()].filter(Boolean).join(" · ") || paper.fileName)}</p>
      </div>
      <div class="viewer-actions">
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
      ${pdfContent}
    </div>
  </section>`;
}

function organizationTemplate(): string {
  if (state.organizationLoading) {
    return `<section class="organization-view organization-loading">
      <span class="reading-spinner"><i></i><i></i><i></i></span>
      <h2>分類候補を生成しています</h2>
      <p>527件のタイトルと既存コレクションを taxonomy.toml のルールと照合しています。</p>
    </section>`;
  }
  const plan = state.organizationPlan;
  if (!plan) {
    return `<section class="organization-view organization-loading"><h2>分類計画を読み込めませんでした</h2><p>ワークスペース設定を確認してください。</p></section>`;
  }

  const folderCounts = new Map<string, number>();
  plan.assignments.forEach((assignment) => assignment.suggestedFolders.forEach((folder) => {
    folderCounts.set(folder, (folderCounts.get(folder) ?? 0) + 1);
  }));
  const folders = [...folderCounts.entries()].sort((left, right) => right[1] - left[1]);
  const coverage = plan.paperCount ? Math.round(plan.classifiedCount / plan.paperCount * 100) : 0;

  return `<section class="organization-view">
    <header class="organization-header">
      <div><p class="section-kicker">BUKAN ORGANIZER</p><h1>文献の整理計画</h1><p>Paperpileへ適用する前の、根拠付きフォルダ・ラベル候補です。</p></div>
      <span class="taxonomy-version">taxonomy v${plan.taxonomyVersion}</span>
    </header>
    <div class="organization-stats">
      <article><small>全文献</small><strong>${plan.paperCount}</strong><span>papers</span></article>
      <article><small>候補あり</small><strong>${plan.classifiedCount}</strong><span>${coverage}% coverage</span></article>
      <article class="review-stat"><small>手動確認</small><strong>${plan.reviewRequiredCount}</strong><span>自動分類なし</span></article>
      <article><small>提案フォルダ</small><strong>${folders.length}</strong><span>${escapeHtml(plan.folderRoot)}/ 以下</span></article>
    </div>
    <div class="organization-columns">
      <section class="taxonomy-panel">
        <div class="organization-section-title"><div><p class="section-kicker">FOLDER MAP</p><h2>${escapeHtml(plan.folderRoot)}/</h2></div><span>${folders.length} folders</span></div>
        <div class="folder-map">
          ${folders.map(([folder, count]) => {
            const relative = folder.startsWith(`${plan.folderRoot}/`) ? folder.slice(plan.folderRoot.length + 1) : folder;
            const [axis, ...rest] = relative.split("/");
            return `<div class="folder-map-row"><span>${icons.folder}</span><p><small>${escapeHtml(axis ?? "")}</small><strong>${escapeHtml(rest.join(" / ") || axis || folder)}</strong></p><em>${count}</em></div>`;
          }).join("")}
        </div>
      </section>
      <section class="assignment-panel">
        <div class="organization-section-title"><div><p class="section-kicker">REVIEW QUEUE</p><h2>分類候補</h2></div><span>上位 ${Math.min(plan.assignments.length, 120)} 件</span></div>
        <div class="assignment-list">
          ${plan.assignments.slice(0, 120).map((assignment) => `
            <article class="assignment-row ${assignment.reviewRequired ? "needs-review" : ""}">
              <div class="assignment-title"><span>${assignment.reviewRequired ? "要確認" : "候補"}</span><h3>${escapeHtml(assignment.title)}</h3><small>${escapeHtml(assignment.currentCollections.map(displayCollectionPath).filter(Boolean).join(" · ") || "未分類")}</small></div>
              <div class="suggestion-chips">
                ${assignment.suggestedFolders.map((folder) => `<i class="folder-chip">${escapeHtml(folder)}</i>`).join("")}
                ${assignment.suggestedLabels.map((label) => `<i>${escapeHtml(label)}</i>`).join("")}
              </div>
              <p class="evidence">${escapeHtml(assignment.evidence.join(" · ") || "分類根拠なし — 手動で確認")}</p>
            </article>`).join("")}
        </div>
      </section>
    </div>
  </section>`;
}

function workspaceTemplate(): string {
  const papers = filteredPapers();
  const currentTitle = state.mode === "organize"
    ? "Bukan 整理"
    : state.collection ?? (state.starredOnly ? "Starred Papers" : "All Papers");
  return `<div class="workspace">
    ${sidebarTemplate()}
    <main class="workspace-main">
      <header class="topbar">
        <div class="breadcrumb"><span>${escapeHtml(state.workspaceName ?? "Paperpile")}</span>${icons.chevron}<strong>${escapeHtml(currentTitle)}</strong></div>
        <div class="topbar-actions">
          ${isDemoMode ? `<span class="demo-badge">DEMO DATA · 4件のみ</span>` : ""}
          ${state.scanning ? `<span class="scan-status"><i></i>ライブラリを読み取り中</span>` : ""}
          ${state.library?.warnings.length ? `<span class="warning-count" title="読み込めなかったファイルがあります">${state.library.warnings.length} warnings</span>` : ""}
          <button class="icon-button refresh-library ${state.scanning ? "spinning" : ""}" title="再読み込み" aria-label="再読み込み">${icons.refresh}</button>
        </div>
      </header>
      ${state.mode === "organize" ? organizationTemplate() : `<div class="content-grid">${paperListTemplate(papers)}${viewerTemplate()}</div>`}
    </main>
  </div>`;
}

function loadingTemplate(message = "Google Drive を探しています"): string {
  return `<main class="loading-screen"><div class="loading-mark"><span>B</span><i></i></div><p>${escapeHtml(message)}</p><small>Paperpile のファイルは変更しません</small></main>`;
}

function render(): void {
  if (state.loading) app.innerHTML = loadingTemplate(state.loadingLabel);
  else if (!state.library) app.innerHTML = onboardingTemplate();
  else app.innerHTML = workspaceTemplate();
  bindEvents();
}

function bindEvents(): void {
  document.querySelectorAll<HTMLElement>("[data-library-path]").forEach((element) => {
    element.addEventListener("click", () => {
      state.workspaceRoot = null;
      state.workspaceName = null;
      void loadLibrary(element.dataset.libraryPath ?? "");
    });
  });
  document.querySelectorAll<HTMLElement>(".choose-folder, .change-library").forEach((element) => {
    element.addEventListener("click", () => void chooseLibrary());
  });
  document.querySelector<HTMLElement>(".open-workspace")?.addEventListener("click", () => void chooseWorkspace(false));
  document.querySelector<HTMLElement>(".create-workspace")?.addEventListener("click", () => void chooseWorkspace(true));
  document.querySelector<HTMLElement>("[data-view='all']")?.addEventListener("click", () => {
    state.mode = "library";
    state.collection = null;
    state.starredOnly = false;
    state.visibleLimit = 120;
    render();
  });
  document.querySelector<HTMLElement>("[data-view='starred']")?.addEventListener("click", () => {
    state.mode = "library";
    state.collection = null;
    state.starredOnly = true;
    state.visibleLimit = 120;
    render();
  });
  document.querySelector<HTMLElement>("[data-view='organize']")?.addEventListener("click", () => void openOrganization());
  document.querySelectorAll<HTMLElement>("[data-collection]").forEach((element) => {
    element.addEventListener("click", () => {
      state.collection = element.dataset.collection ?? null;
      state.mode = "library";
      state.starredOnly = false;
      state.visibleLimit = 120;
      render();
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
      state.selectedId = element.dataset.paperId ?? null;
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
  const pdfObject = document.querySelector<HTMLObjectElement>("#pdf-object");
  const pdfLoading = document.querySelector<HTMLElement>("#pdf-loading");
  if (pdfObject && pdfLoading) {
    const finishPdfLoad = () => pdfLoading.classList.add("loaded");
    pdfObject.addEventListener("load", finishPdfLoad, { once: true });
    pdfObject.addEventListener("error", () => {
      pdfLoading.classList.add("failed");
      const title = pdfLoading.querySelector("strong");
      const detail = pdfLoading.querySelector("small");
      if (title) title.textContent = "プレビューを読み込めませんでした";
      if (detail) detail.textContent = "「別ウィンドウで開く」をお試しください";
    }, { once: true });
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

async function openOrganization(): Promise<void> {
  if (!state.workspaceRoot) {
    showToast("Bukan整理を使うには、先にワークスペースを開いてください", true);
    return;
  }
  state.mode = "organize";
  if (state.organizationPlan) {
    render();
    return;
  }
  state.organizationLoading = true;
  render();
  try {
    state.organizationPlan = await invoke<OrganizationPlan>("suggest_organization", { workspaceRoot: state.workspaceRoot });
  } catch (error) {
    showToast(String(error), true);
  } finally {
    state.organizationLoading = false;
    render();
  }
}

async function chooseLibrary(): Promise<void> {
  try {
    const selected = await open({ directory: true, multiple: false, title: "Paperpile フォルダを選択" });
    if (selected) {
      state.workspaceRoot = null;
      state.workspaceName = null;
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
  state.workspaceRoot = descriptor.root;
  state.workspaceName = descriptor.name;
  state.organizationPlan = null;
  localStorage.setItem("bukan.workspaceRoot", descriptor.root);
  await loadLibrary(descriptor.paperpileRoot);
}

async function loadLibrary(root: string, quiet = false): Promise<void> {
  if (!root) return;
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
    state.library = index;
    state.selectedId = state.selectedId && index.papers.some((paper) => paper.id === state.selectedId) ? state.selectedId : null;
    localStorage.setItem("bukan.paperpileRoot", index.root);
    if (quiet) showToast(`${index.stats.paperCount} 件の文献を更新しました`);
  } catch (error) {
    state.error = String(error);
    if (!quiet) state.library = null;
  } finally {
    state.loading = false;
    state.scanning = false;
    render();
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

async function initialize(): Promise<void> {
  if (isDemoMode) {
    state.library = demoLibrary();
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

void initialize();
