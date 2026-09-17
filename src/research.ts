import { invoke } from "@tauri-apps/api/core";
import { escapeHtml, hydrateResearchMarkdown, renderResearchMarkdown } from "./markdown";
import "./research.css";

interface ResearchRuntime { available: boolean; ready: boolean; detail: string }
interface ResearchDocument { path: string; title: string; group: string; modifiedAt: number }
interface WorkspaceStatus {
  workspaceRoot: string;
  name: string;
  storePath: string;
  runtime: ResearchRuntime;
  documents: ResearchDocument[];
  warnings?: string[];
}
interface RecordSummary {
  id: string;
  revision: number;
  kind: string;
  title: string;
  summary: string;
  updatedAt: string;
  state?: string;
  readingStatus?: string;
}
interface RecordPage {
  info: { counts?: Record<string, number>; revisions?: number };
  records: RecordSummary[];
  hasMore: boolean;
  nextOffset: number | null;
}
interface RecordDetail {
  id: string;
  revision: number;
  kind: string;
  title: string;
  markdown: string;
  editable: boolean;
  editText?: string;
  editPath?: string;
  path?: string;
  references: { id: string; revision: number }[];
  previewWarning?: string;
  wiki?: WikiDetail;
}
interface WikiReference { id: string; revision: number; title?: string; sectionId?: string; relation?: string; previewPath?: string }
interface FrozenReference { id: string; revision: number; previewPath: string; contextId?: string; contextRevision?: number; contextTitle?: string }
interface WikiSection { id: string; title: string; markdown: string; previewMarkdown?: string; basis: WikiReference[]; reviewStatus?: string; interpretation?: string; audit?: { reviewer: string; reviewed_revision: number; reviewed_at: string; findings: string }[] }
interface WikiPageSummary { id: string; revision: number; title: string; summary: string; pageType: string; parentIds: string[]; categories: string[]; aliases: string[]; updatedAt: string }
interface WikiTask { id: string; title: string; state: string; reason?: string; pageId?: string; sectionId?: string }
interface WikiCandidate { id: string; title: string; state: string; reason?: string; pageIds?: string[]; recordId?: string; recordRevision?: number }
interface WikiHome { pages: WikiPageSummary[]; tasks: WikiTask[]; candidates: WikiCandidate[]; hasMore?: boolean; nextOffset?: number | null; formatVersion?: number }
interface WikiDetail {
  summary: string; previewSummary?: string; pageType: string; parentIds: string[]; categories: string[]; aliases: string[];
  sections: WikiSection[]; navigation: { id: string; title: string }[];
  backlinks: { id: string; title: string }[];
  history: { revision: number; createdAt: string; author: string; changeReason: string }[];
  warnings: { sectionId?: string; reason: string }[];
  searchThrough?: string; verifiedAt?: string;
  frozenReferences?: FrozenReference[];
}
interface WikiEdit { title: string; summary: string; sections: { id: string; title: string; markdown: string }[]; changeReason: string }
interface FileDetail { path: string; relativePath: string; title: string; markdown: string; sha256: string; editable: boolean }
type Selection = { type: "record"; value: RecordDetail; pinned: boolean } | { type: "file"; value: FileDetail };
interface Draft { text: string; original: string; editing: boolean; source: Selection; baseSha256?: string; wiki?: WikiEdit }
interface ResearchCallbacks { render: () => void; openCodex: () => Promise<void>; openPaper: (path: string, page?: number) => void }
interface ResearchState {
  root: string;
  name: string | null;
  status: WorkspaceStatus | null;
  page: RecordPage | null;
  pageCriteria: { query: string; kind: string } | null;
  loaded: boolean;
  loading: boolean;
  loadingRecords: boolean;
  loadingDocument: boolean;
  preparing: boolean;
  saving: boolean;
  savingRequest: boolean;
  error: string;
  message: string;
  query: string;
  kind: string;
  tab: "wiki" | "records" | "documents";
  wiki: WikiHome | null;
  wikiStatus: { formatVersion: number; needsMigration: boolean } | null;
  wikiLoading: boolean;
  wikiGeneration: number;
  wikiType: string;
  wikiCategory: string;
  wikiView: "pages" | "updates";
  history: boolean;
  selectedSection: string | null;
  wikiReturn: { id: string; revision: number; sectionId?: string } | null;
  frozenChoices: { source: Selection; references: FrozenReference[]; fragment?: string } | null;
  creatingWiki: boolean;
  newWiki: { title: string; pageType: string; parent: { id: string; title: string } | null } | null;
  selected: Selection | null;
  drafts: Map<string, Draft>;
  request: string;
  requestLoaded: boolean;
  requestChanges: number;
  requestSaved: boolean;
  requestTask: { key: string; id: string } | null;
  queryGeneration: number;
  documentGeneration: number;
  scrollTop: number;
  catalogScrollTop: number;
  focus: { id: string; start: number | null; end: number | null; scrollTop: number } | null;
  demo: boolean;
}

const kinds = [
  ["", "すべて"], ["wiki_page", "研究wiki"], ["question", "問い・課題"], ["topic", "研究テーマ"], ["paper_note", "文献ノート"],
  ["paper", "論文"], ["claim", "主張"], ["relation", "関係・比較"], ["review_task", "レビュー作業"],
  ["evidence", "根拠"], ["source", "原文記録"],
];
const wikiTypes = [["portal", "分野の入口"], ["theme", "研究テーマ"], ["concept", "概念・現象"], ["model", "理論・モデル"], ["method", "手法"], ["material", "対象・資料"], ["question", "問い・論争"], ["comparison", "条件別の比較"], ["history", "研究史"], ["glossary", "用語一覧"]];
const workspaceStates = new Map<string, ResearchState>();
let state: ResearchState | null = null;
let callbacks: ResearchCallbacks | null = null;

function newState(root: string, name: string | null): ResearchState {
  return {
    root, name, status: null, page: null, pageCriteria: null, loaded: false, loading: false, loadingRecords: false,
    loadingDocument: false, preparing: false, saving: false, savingRequest: false,
    error: "", message: "", query: "", kind: "", tab: "wiki", selected: null, drafts: new Map(),
    wiki: null, wikiStatus: null, wikiLoading: false, wikiGeneration: 0, wikiType: "", wikiCategory: "", wikiView: "pages", history: false, selectedSection: null, wikiReturn: null, frozenChoices: null, creatingWiki: false, newWiki: null,
    request: "", requestLoaded: false, requestChanges: 0, requestSaved: false, requestTask: null,
    queryGeneration: 0, documentGeneration: 0, scrollTop: 0, catalogScrollTop: 0, focus: null, demo: false,
  };
}

export function setResearchWorkspace(root: string | null, name: string | null): void {
  if (!root) { state = null; return; }
  let existing = workspaceStates.get(root);
  if (!existing) { existing = newState(root, name); workspaceStates.set(root, existing); }
  existing.name = name ?? existing.name;
  state = existing;
}

function render(): void { callbacks?.render(); }
function isCurrent(current: ResearchState): boolean { return state === current; }
function kindLabel(kind: string): string { return kinds.find(([value]) => value === kind)?.[1] ?? kind; }
function wikiTypeLabel(kind: string): string { return wikiTypes.find(([value]) => value === kind)?.[1] ?? kind; }
function workStateLabel(status: string): string { return ({ pending: "未処理", running: "進行中", completed: "完了", needs_followup: "追加対応が必要", accepted: "関連付け済み・未統合", rejected: "対象外と判断", integrated: "統合済み" })[status] ?? status; }
function wikiEditFor(value: RecordDetail): WikiEdit | undefined {
  if (!value.wiki) return undefined;
  return { title: value.title, summary: value.wiki.summary, sections: value.wiki.sections.map(({ id, title, markdown }) => ({ id, title, markdown })), changeReason: "" };
}
function keyFor(selected: Selection): string {
  return selected.type === "record" ? `record:${selected.value.id}@${selected.value.revision}` : `file:${selected.value.path}`;
}
function draftFor(current: ResearchState, selected: Selection): Draft {
  const key = keyFor(selected);
  let draft = current.drafts.get(key);
  if (!draft) {
    const wiki = selected.type === "record" ? wikiEditFor(selected.value) : undefined;
    const text = wiki ? JSON.stringify(wiki) : selected.type === "record" ? selected.value.editText ?? selected.value.markdown : selected.value.markdown;
    draft = { text, original: text, editing: false, source: selected, wiki, baseSha256: selected.type === "file" ? selected.value.sha256 : undefined };
    current.drafts.set(key, draft);
  } else if (selected.type === "file" && draft.baseSha256 !== selected.value.sha256 && draft.text === draft.original) {
    draft.text = selected.value.markdown;
    draft.original = selected.value.markdown;
    draft.baseSha256 = selected.value.sha256;
    draft.source = selected;
  }
  return draft;
}
function previewPath(current: ResearchState, selected: Selection): string | undefined {
  const draft = draftFor(current, selected);
  return selected.type === "record" && draft.text !== draft.original && !selected.pinned ? selected.value.editPath : selected.value.path;
}
function button(action: string, label: string, disabled = false, primary = false): string {
  return `<button type="button" class="research-button${primary ? " primary" : ""}" data-research-action="${action}"${disabled ? " disabled" : ""}>${label}</button>`;
}

export function researchTemplate(): string {
  const current = state;
  if (!current) return `<section class="research-home"><div class="research-empty"><h1>研究wiki</h1><p>研究ワークスペースを開くと、概念・問いごとの現在の理解から、比較・文献ノート・原資料へ進めます。</p><p>サイドバーからワークスペースを開くか、新しく作成してください。</p></div></section>`;
  const previous = document.querySelector<HTMLElement>(".research-home");
  if (previous?.dataset.researchWorkspace === current.root) {
    current.scrollTop = previous.scrollTop;
    current.catalogScrollTop = previous.querySelector(".research-catalog-results")?.scrollTop ?? 0;
    const focused = document.activeElement;
    current.focus = focused && previous.contains(focused) && (focused instanceof HTMLTextAreaElement || focused instanceof HTMLInputElement) ? { id: focused.id, start: focused.selectionStart, end: focused.selectionEnd, scrollTop: focused.scrollTop } : null;
  }
  const runtime = current.status?.runtime;
  const counts = current.page?.info.counts ?? {};
  return `<section class="research-home" aria-label="研究wiki" data-research-workspace="${escapeHtml(current.root)}">
    <header class="research-header"><div><p class="section-kicker">研究 WIKI${current.demo ? " · DEMO" : ""}</p><h1>${escapeHtml(current.status?.name ?? current.name ?? "研究ワークスペース")}</h1><p class="research-path" title="${escapeHtml(current.root)}">${escapeHtml(current.root)}</p></div>${button("refresh", current.loading ? "更新中…" : "更新", current.loading || current.saving || current.loadingDocument || current.demo)}</header>
    ${current.error ? `<div class="research-notice error" role="alert">${escapeHtml(current.error)}</div>` : ""}
    ${current.message ? `<div class="research-notice" role="status">${escapeHtml(current.message)}</div>` : ""}
    ${current.frozenChoices?.source === current.selected ? `<div class="research-notice"><p>この資料は複数の項目で異なる画像を採用しています。確認する保存版を選んでください。</p><div class="research-detail-actions">${current.frozenChoices.references.map((reference, index) => `<button type="button" class="research-button" data-research-frozen="${index}">${escapeHtml(reference.contextTitle ?? `採用先 ${index + 1}`)}${reference.contextRevision ? ` · 保存版 ${reference.contextRevision}` : ""}</button>`).join("")}</div></div>` : ""}
    ${current.status?.warnings?.length ? `<details class="research-notice research-warnings"><summary>資料の読み込みに関するお知らせ（${current.status.warnings.length}件）</summary><ul>${current.status.warnings.map((warning) => `<li>${escapeHtml(warning)}</li>`).join("")}</ul></details>` : ""}
    ${runtime && !runtime.ready ? `<div class="research-setup"><div><strong>研究機能の準備</strong><p>${escapeHtml(runtime.detail)}</p><small>初回はインターネット接続が必要です。保存済みのレポートは準備前でも開けます。</small></div>${button("prepare", current.preparing ? "準備中…" : "研究機能を準備", current.preparing || current.demo, true)}</div>` : ""}
    ${current.wikiStatus?.needsMigration ? `<div class="research-setup"><div><strong>研究wikiを使うための保存形式の更新</strong><p>既存の研究記録を保持し、バックアップを作成して形式2へ移行します。</p><small>移行後は、この形式に対応するBukanで開いてください。</small></div>${button("wiki-migrate", current.wikiLoading ? "移行中…" : "バックアップして移行", current.wikiLoading || current.saving || current.demo, true)}</div>` : ""}
    <div class="research-overview">
      <form class="research-request" id="research-request-form"><label for="research-request-text">この項目から調査を続ける</label>${current.selected ? `<p class="research-request-context">対象：${escapeHtml(current.selected.value.title)}${current.selectedSection && current.selected.type === "record" ? ` / ${escapeHtml(current.selected.value.wiki?.sections.find((section) => section.id === current.selectedSection)?.title ?? "選択した節")}` : ""}</p>` : ""}<textarea id="research-request-text" rows="2" placeholder="例：この説明の根拠を検証し、離脱条件の比較を更新して。">${escapeHtml(current.request)}</textarea><div class="research-request-actions"><span>対象ページ・節・採用した根拠を保存してCodexへ引き継ぎます。</span><button type="submit" class="research-button primary"${current.savingRequest || current.demo ? " disabled" : ""}>${current.savingRequest ? "保存中…" : "依頼を保存してCodexを開く"}</button></div>${current.requestSaved ? `<div class="research-handoff"><code>.bukan/current-research.md を読んで、依頼を進めて</code>${button("copy-request", "指示をコピー")}<small>Codexにこの指示を入力すると作業を開始できます。</small></div>` : ""}</form>
      <div class="research-counts"><button type="button" data-research-action="wiki-home"><strong>${counts["wiki_page"] ?? "—"}</strong><span>研究wikiの項目</span></button><button type="button" data-research-action="wiki-updates"><strong>${current.wiki?.tasks.filter((task) => !["completed", "resolved", "dismissed"].includes(task.state)).length ?? "—"}</strong><span>再検討・調査待ち</span></button><button type="button" data-research-action="wiki-updates"><strong>${current.wiki?.candidates.filter((candidate) => candidate.state === "pending").length ?? "—"}</strong><span>未統合の候補</span></button>${[["paper", "論文"], ["paper_note", "文献ノート"], ["claim", "主張"]].map(([kind, label]) => `<button type="button" data-research-kind="${kind}"><strong>${counts[kind ?? ""] ?? "—"}</strong><span>${label}</span></button>`).join("")}<small>保存された記録数です。文献探索の網羅率や科学的な確かさを表す数値ではありません。</small></div>
    </div>
    <div class="research-browser">
      <section class="research-catalog" aria-label="研究資料"><div class="research-tabs"><button type="button" data-research-tab="wiki" aria-pressed="${current.tab === "wiki"}">研究wiki</button><button type="button" data-research-tab="records" aria-pressed="${current.tab === "records"}">研究記録</button><button type="button" data-research-tab="documents" aria-pressed="${current.tab === "documents"}">ノート・レポート</button></div>
        <form class="research-search" id="research-search-form"><label class="research-sr-only" for="research-query">研究資料を検索</label><input id="research-query" type="search" value="${escapeHtml(current.query)}" placeholder="${current.tab === "wiki" ? "項目名・別名・本文を検索" : "主張・課題・ノートを検索"}"><button type="submit" class="research-button"${current.loadingRecords || current.wikiLoading ? " disabled" : ""}>検索</button>${current.tab === "records" ? `<label class="research-sr-only" for="research-kind">記録の種類</label><select id="research-kind">${kinds.map(([kind, label]) => `<option value="${kind}"${current.kind === kind ? " selected" : ""}>${label}</option>`).join("")}</select>` : current.tab === "wiki" ? wikiFiltersTemplate(current) : ""}</form>
        ${current.tab === "wiki" ? `<div class="research-wiki-tools">${button("wiki-home", "項目の入口")}${button("wiki-updates", "調査待ち")}${button("wiki-create", "＋ 項目", current.creatingWiki || !current.status?.runtime.ready || current.wikiStatus?.needsMigration || current.demo)}</div>` : ""}
        ${draftsTemplate(current)}<div class="research-catalog-results" aria-busy="${current.loadingRecords || current.wikiLoading}">${current.tab === "wiki" ? wikiCatalogTemplate(current) : current.tab === "records" ? recordsTemplate(current) : documentsTemplate(current)}</div>
      </section>
      <section class="research-detail" aria-label="選択した研究資料"${current.loadingDocument ? ' aria-busy="true"' : ""}>${detailTemplate(current)}</section>
    </div>
  </section>`;
}

function draftsTemplate(current: ResearchState): string {
  const drafts = [...current.drafts.entries()].filter(([, draft]) => draft.text !== draft.original);
  if (!drafts.length) return "";
  return `<div class="research-drafts"><strong>未保存の下書き</strong>${drafts.map(([key, draft]) => `<button type="button" data-research-draft="${escapeHtml(key)}"${current.saving || current.loadingDocument ? " disabled" : ""}>${escapeHtml(draft.source.value.title)}${draft.source.type === "record" ? ` · r${draft.source.value.revision}` : ""}</button>`).join("")}</div>`;
}

function wikiFiltersTemplate(current: ResearchState): string {
  const categories = [...new Set([...(current.wiki?.pages.flatMap((page) => page.categories) ?? []), ...(current.wikiCategory ? [current.wikiCategory] : [])])].sort();
  return `<label class="research-sr-only" for="research-wiki-type">項目の種類</label><select id="research-wiki-type"><option value="">すべての種類</option>${wikiTypes.map(([type, label]) => `<option value="${type}"${type === current.wikiType ? " selected" : ""}>${label}</option>`).join("")}</select><label class="research-sr-only" for="research-wiki-category">分類</label><select id="research-wiki-category"><option value="">すべての分類</option>${categories.map((category) => `<option${category === current.wikiCategory ? " selected" : ""}>${escapeHtml(category)}</option>`).join("")}</select>`;
}

function wikiCatalogTemplate(current: ResearchState): string {
  if (current.wikiLoading && !current.wiki) return `<p class="research-empty-text">研究wikiを読み込み中…</p>`;
  if (!current.status?.runtime.ready && !current.demo) return `<p class="research-empty-text">研究機能を準備すると、研究wikiを作成・閲覧できます。</p>`;
  const pages = (current.wiki?.pages ?? []).filter((page) => (!current.wikiType || page.pageType === current.wikiType) && (!current.wikiCategory || page.categories.includes(current.wikiCategory)));
  if (!pages.length) return `<p class="research-empty-text">${current.query || current.wikiType || current.wikiCategory ? "条件に一致する項目がありません。研究記録から主張や文献ノートも検索できます。" : "研究wikiはまだありません。「＋ 項目」から、対象や問いのページを作成できます。"}</p>`;
  const pageIds = new Set(pages.map((page) => page.id));
  const children = new Map<string, WikiPageSummary[]>();
  for (const page of pages) for (const parentId of page.parentIds) {
    const siblings = children.get(parentId) ?? [];
    siblings.push(page);
    children.set(parentId, siblings);
  }
  const expanded = new Set<string>();
  const visit = (page: WikiPageSummary, depth: number): string => {
    const expandChildren = !expanded.has(page.id);
    expanded.add(page.id);
    return `<button type="button" class="research-record research-wiki-record${current.selected?.type === "record" && current.selected.value.id === page.id ? " selected" : ""}" style="--wiki-depth:${Math.min(depth, 4)}" data-research-record="${escapeHtml(page.id)}"${current.saving ? " disabled" : ""}><span class="research-record-meta">${escapeHtml(wikiTypeLabel(page.pageType))}</span><strong>${escapeHtml(page.title)}</strong>${page.summary ? `<p>${escapeHtml(page.summary)}</p>` : ""}${page.categories.length ? `<small>${page.categories.map(escapeHtml).join(" · ")}</small>` : ""}</button>${expandChildren ? (children.get(page.id) ?? []).map((child) => visit(child, depth + 1)).join("") : ""}`;
  };
  return pages.filter((page) => !page.parentIds.some((parent) => pageIds.has(parent))).map((page) => visit(page, 0)).join("") + pages.map((page) => expanded.has(page.id) ? "" : visit(page, 0)).join("");
}

function wikiCreateTemplate(current: ResearchState): string {
  const draft = current.newWiki;
  if (!draft) return "";
  const parents = (current.wiki?.pages ?? []).map(({ id, title }) => ({ id, title }));
  if (draft.parent && !parents.some((parent) => parent.id === draft.parent?.id)) parents.push(draft.parent);
  return `<form id="research-wiki-create-form" class="research-wiki-editor"><h2>研究wikiの項目を作成</h2><p>継続して育てる対象や問いの名前を付けてください。</p><label for="research-wiki-new-title">項目名</label><input id="research-wiki-new-title" required maxlength="180" value="${escapeHtml(draft.title)}" placeholder="例：表面からの離脱"><label for="research-wiki-new-type">種類</label><select id="research-wiki-new-type">${wikiTypes.map(([type, label]) => `<option value="${type}"${draft.pageType === type ? " selected" : ""}>${label}</option>`).join("")}</select><label for="research-wiki-new-parent">上位の項目</label><select id="research-wiki-new-parent"><option value=""${draft.parent ? "" : " selected"}>分野の入口に置く</option>${parents.map((page) => `<option value="${escapeHtml(page.id)}"${draft.parent?.id === page.id ? " selected" : ""}>${escapeHtml(page.title)}</option>`).join("")}</select><div class="research-detail-actions"><button type="submit" class="research-button primary"${current.creatingWiki ? " disabled" : ""}>${current.creatingWiki ? "作成中…" : "作成して編集"}</button>${button("wiki-create-cancel", "取り消す", current.creatingWiki)}</div></form>`;
}

function wikiHomeTemplate(current: ResearchState): string {
  const pages = current.wiki?.pages ?? [];
  const roots = pages.filter((page) => !page.parentIds.length);
  const updated = [...pages].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)).slice(0, 5);
  return `<header class="research-detail-header"><span class="research-record-meta">現在の理解から、条件・比較・原資料へ</span><h2>研究wikiの入口</h2></header><p class="research-wiki-intro">同じ項目を継続して改訂し、根拠と留保を残します。詳しい解析や文献ごとの読解は、各項目からたどれます。</p>${!pages.length ? `<div class="research-empty"><h3>最初の項目を作る</h3><p>研究対象や問いから始め、根拠を確認した内容を節ごとに追加できます。</p>${button("wiki-create", "項目を作成", !current.status?.runtime.ready || current.wikiStatus?.needsMigration || current.demo, true)}</div>` : `<div class="research-wiki-grid">${roots.map((page) => `<button type="button" data-research-record="${escapeHtml(page.id)}"${current.saving ? " disabled" : ""}><span>${escapeHtml(wikiTypeLabel(page.pageType))}</span><strong>${escapeHtml(page.title)}</strong><p>${escapeHtml(page.summary)}</p><small>${pages.filter((child) => child.parentIds.includes(page.id)).length} 件の下位項目</small></button>`).join("")}</div><h3>最近更新した項目</h3><div class="research-wiki-links">${updated.map((page) => `<button type="button" data-research-record="${escapeHtml(page.id)}"${current.saving ? " disabled" : ""}>${escapeHtml(page.title)}<small>${escapeHtml(page.updatedAt.slice(0, 10))}</small></button>`).join("")}</div>`}`;
}

function wikiUpdatesTemplate(current: ResearchState): string {
  const tasks = current.wiki?.tasks ?? [];
  const candidates = current.wiki?.candidates ?? [];
  return `<header class="research-detail-header"><span class="research-record-meta">再検討と未統合資料</span><h2>調査待ち</h2></header><p class="research-wiki-intro">根拠の変更と新しい資料を分けて確認します。候補に入ったことは、関連性や科学的な妥当性の確認を意味しません。</p><h3>再検討・更新作業</h3>${tasks.length ? tasks.map((task) => `<div class="research-wiki-update"><span class="research-record-meta">${escapeHtml(workStateLabel(task.state))}</span><strong>${escapeHtml(task.title)}</strong>${task.reason ? `<p>${escapeHtml(task.reason)}</p>` : ""}<div class="research-detail-actions"><button type="button" class="research-button" data-research-record="${escapeHtml(task.id)}">作業記録</button>${task.pageId ? `<button type="button" class="research-button" data-research-record="${escapeHtml(task.pageId)}"${task.sectionId ? ` data-research-section="${escapeHtml(task.sectionId)}"` : ""}>対象の項目</button>` : ""}</div></div>`).join("") : '<p class="research-empty-text">保存された更新作業はありません。</p>'}<h3>新しい資料・未統合の候補</h3>${candidates.length ? candidates.map((candidate) => `<div class="research-wiki-update"><span class="research-record-meta">${escapeHtml(workStateLabel(candidate.state))}</span><strong>${escapeHtml(candidate.title)}</strong>${candidate.reason ? `<p>${escapeHtml(candidate.reason)}</p>` : ""}<div class="research-detail-actions">${candidate.recordId ? `<button type="button" class="research-button" data-research-record="${escapeHtml(candidate.recordId)}"${candidate.recordRevision ? ` data-research-revision="${candidate.recordRevision}"` : ""}>資料を確認</button>` : ""}${(candidate.pageIds ?? []).map((id) => `<button type="button" class="research-button" data-research-record="${escapeHtml(id)}">${escapeHtml(current.wiki?.pages.find((page) => page.id === id)?.title ?? "関連する項目")}</button>`).join("")}</div></div>`).join("") : '<p class="research-empty-text">保存された候補はありません。探索の完了を表すものではありません。</p>'}`;
}

function recordsTemplate(current: ResearchState): string {
  if (current.loadingRecords && !current.page) return `<p class="research-empty-text">研究記録を読み込み中…</p>`;
  if (!current.status?.runtime.ready && !current.demo) return `<p class="research-empty-text">研究機能を準備すると、主張・根拠・関係を検索できます。</p>`;
  const records = current.page?.records ?? [];
  if (!records.length) return `<p class="research-empty-text">${current.query || current.kind ? "条件に一致する記録がありません。" : "研究記録はまだありません。調べたいことをCodexに伝えて研究を始めてください。"}</p>`;
  return records.map((record) => `<button type="button" class="research-record${current.selected?.type === "record" && current.selected.value.id === record.id ? " selected" : ""}" data-research-record="${escapeHtml(record.id)}"${current.saving ? " disabled" : ""}><span class="research-record-meta"><span>${escapeHtml(kindLabel(record.kind))}</span><span>r${record.revision}${record.state ? ` · ${escapeHtml(record.state)}` : ""}</span></span><strong>${escapeHtml(record.title || record.id)}</strong>${record.summary ? `<p>${escapeHtml(record.summary)}</p>` : ""}${record.readingStatus ? `<small>${escapeHtml(record.readingStatus)}</small>` : ""}</button>`).join("") + (current.page?.hasMore ? `<div class="research-more">${button("more", current.loadingRecords ? "読み込み中…" : "さらに表示", current.loadingRecords)}</div>` : "");
}

function documentsTemplate(current: ResearchState): string {
  const query = current.query.toLocaleLowerCase();
  const documents = (current.status?.documents ?? []).filter((item) => `${item.title} ${item.path}`.toLocaleLowerCase().includes(query));
  if (!documents.length) return `<p class="research-empty-text">${query ? "一致するファイルがありません。" : "保存されたノート・レポートはまだありません。"}</p>`;
  return documents.map((item) => `<button type="button" class="research-record${current.selected?.type === "file" && current.selected.value.relativePath === item.path ? " selected" : ""}" data-research-document="${escapeHtml(item.path)}"${current.saving ? " disabled" : ""}><span class="research-record-meta">${escapeHtml(item.group)}</span><strong>${escapeHtml(item.title)}</strong><small>${escapeHtml(item.path)}</small></button>`).join("");
}

function detailTemplate(current: ResearchState): string {
  if (current.newWiki) return wikiCreateTemplate(current);
  if (current.tab === "wiki" && current.wikiView === "updates") return wikiUpdatesTemplate(current);
  const selected = current.selected;
  if (!selected && current.tab === "wiki") return wikiHomeTemplate(current);
  if (!selected) return `<div class="research-empty"><span class="research-empty-mark">文</span><h2>${current.loadingDocument ? "資料を読み込み中…" : "問いから、根拠まで"}</h2><p>研究記録やレポートを選ぶと、本文・数式・図表をここで読めます。</p><p>文献ノートへの追記は研究記録として保存され、次の検索と解析に使われます。</p></div>`;
  if (selected.type === "record" && selected.value.wiki) return wikiDetailTemplate(current, selected);
  const value = selected.value;
  const draft = draftFor(current, selected);
  const pinned = selected.type === "record" && selected.pinned;
  const editable = value.editable && !pinned;
  const dirty = !pinned && draft.text !== draft.original;
  const markdown = dirty ? draft.text : value.markdown;
  const preview = selected.type === "file" && !/\.md$/i.test(selected.value.relativePath) ? `<pre><code>${escapeHtml(markdown)}</code></pre>` : renderResearchMarkdown(markdown);
  const conflict = selected.type === "file" && draft.baseSha256 !== selected.value.sha256;
  const subtitle = selected.type === "record" ? `${kindLabel(selected.value.kind)} · r${selected.value.revision} · ${selected.value.id}` : selected.value.relativePath;
  return `${current.wikiReturn ? `<div class="research-wiki-breadcrumb">${button("wiki-return", "← 参照元のwikiへ戻る", current.saving)}</div>` : ""}<header class="research-detail-header"><div><span class="research-record-meta">${escapeHtml(subtitle)}</span><h2>${escapeHtml(value.title)}</h2></div><div class="research-detail-actions">${editable ? button("edit", draft.editing ? "プレビュー" : "編集", current.saving || current.loadingDocument) : '<span class="research-readonly">閲覧用</span>'}${dirty ? button("save", current.saving ? "保存中…" : "変更を保存", current.saving || current.loadingDocument || current.demo, true) : ""}${dirty ? button("discard", "変更を取り消す", current.saving || current.loadingDocument) : ""}${pinned && selected.type === "record" ? `<button type="button" class="research-button" data-research-record="${escapeHtml(selected.value.id)}">現在版を開く</button>` : ""}</div></header>
    ${pinned ? '<p class="research-notice">根拠として採用された版を表示しています。未保存の下書きは別に保持しています。</p>' : ""}
    ${selected.type === "record" && selected.value.previewWarning ? `<p class="research-notice">${escapeHtml(selected.value.previewWarning)}</p>` : ""}
    ${dirty ? '<p class="research-draft-status" role="status">未保存の変更があります。プレビューには編集中の内容を表示しています。</p>' : ""}
    ${conflict ? '<p class="research-notice error">このファイルは別の操作で更新されました。編集中の内容は保持しています。変更をコピーしてから取り消すと最新版を確認できます。</p>' : ""}
    ${editable ? `<p class="research-edit-help">${selected.type === "record" ? "保存すると文献ノートの新しい改訂が作られ、検索に反映されます。" : "このファイルを更新します。構造化した研究記録とは別に保存されます。"}</p>` : selected.type === "file" && selected.value.relativePath.replaceAll("\\", "/").startsWith("data/") ? '<p class="research-edit-help">書き出された記録は閲覧用です。追記は「研究記録」の文献ノートから保存してください。</p>' : ""}
    ${editable && draft.editing ? `<label class="research-sr-only" for="research-note-editor">本文を編集</label><textarea class="research-note-editor" id="research-note-editor" spellcheck="false">${escapeHtml(draft.text)}</textarea>` : `<article class="research-markdown" id="research-markdown">${preview}</article>`}
    ${selected.type === "record" && selected.value.references.length ? `<footer class="research-references"><h3>根拠・関連記録</h3>${selected.value.references.map((reference) => `<button type="button" data-research-record="${escapeHtml(reference.id)}" data-research-revision="${reference.revision}"${current.saving ? " disabled" : ""}>${escapeHtml(reference.id)} <span>r${reference.revision}</span></button>`).join("")}</footer>` : ""}`;
}

function wikiEditorTemplate(draft: WikiEdit): string {
  return `<div class="research-wiki-editor"><label for="research-wiki-title">項目名</label><input id="research-wiki-title" data-wiki-field="title" value="${escapeHtml(draft.title)}"><label for="research-wiki-summary">現在の理解と主な留保</label><textarea id="research-wiki-summary" data-wiki-field="summary" rows="4">${escapeHtml(draft.summary)}</textarea>${draft.sections.map((section, index) => `<fieldset><legend>節 ${index + 1}</legend><label for="research-wiki-section-title-${index}">見出し</label><input id="research-wiki-section-title-${index}" data-wiki-section-index="${index}" data-wiki-field="title" value="${escapeHtml(section.title)}"><label for="research-wiki-section-text-${index}">本文（Markdown）</label><textarea id="research-wiki-section-text-${index}" class="research-note-editor" data-wiki-section-index="${index}" data-wiki-field="markdown" spellcheck="false">${escapeHtml(section.markdown)}</textarea></fieldset>`).join("")}${button("wiki-add-section", "＋ 節を追加")}<label for="research-wiki-change-reason">変更理由</label><textarea id="research-wiki-change-reason" data-wiki-field="changeReason" rows="2" placeholder="何を、なぜ変更したか">${escapeHtml(draft.changeReason)}</textarea></div>`;
}

function wikiDetailTemplate(current: ResearchState, selected: Extract<Selection, { type: "record" }>): string {
  const value = selected.value;
  const wiki = value.wiki;
  if (!wiki) return "";
  const draft = draftFor(current, selected);
  const dirty = !selected.pinned && draft.text !== draft.original;
  const displayDraft = selected.pinned ? undefined : draft.wiki;
  const editing = draft.editing && !selected.pinned;
  const sections = displayDraft?.sections ?? wiki.sections;
  const labelFor = (id: string) => current.wiki?.pages.find((page) => page.id === id)?.title ?? "関連する項目";
  const navigate = (id: string, title: string) => `<button type="button" data-research-record="${escapeHtml(id)}"${current.saving ? " disabled" : ""}>${escapeHtml(title)}</button>`;
  const reviewLabel = (status: string | undefined) => ({ independently_reviewed: "独立点検の記録あり", unverified: "未検証", migration_unverified: "移行した記述・検証待ち", issues_open: "指摘未解消" })[status ?? ""] ?? status ?? "未検証";
  const relationLabel = (role: string | undefined) => ({ supports: "支持", challenges: "反対の証拠", qualifies: "留保", context: "背景", derives_from: "解釈の出所" })[role ?? ""] ?? role ?? "根拠";
  const interpretationLabel = (origin: string | undefined) => ({ author_explicit: "著者が述べた説明", analyst_inference: "読解担当による統合・解釈", human_proposal: "人間の研究案" })[origin ?? ""] ?? "解釈の出所は未記録";
  return `<nav class="research-wiki-breadcrumb" aria-label="項目の現在地">${button("wiki-home", "研究wiki")}${wiki.parentIds.map((id) => navigate(id, labelFor(id))).join("<span> / </span>")}</nav><header class="research-detail-header"><span class="research-record-meta">${escapeHtml(wikiTypeLabel(wiki.pageType))}${wiki.categories.length ? ` · ${wiki.categories.map(escapeHtml).join(" / ")}` : ""}</span><h2>${escapeHtml(displayDraft?.title ?? value.title)}</h2>${wiki.aliases.length ? `<p class="research-wiki-aliases">別名：${wiki.aliases.map(escapeHtml).join("、")}</p>` : ""}<div class="research-detail-actions">${button("wiki-read", "読む", current.saving)}${!selected.pinned && value.editable ? button("edit", editing ? "プレビュー" : "編集", current.saving || current.loadingDocument) : ""}${button("wiki-history", "変更履歴", current.saving)}${dirty && !selected.pinned ? button("save", current.saving ? "保存中…" : "変更を保存", current.saving || current.loadingDocument || current.demo, true) : ""}${dirty ? button("discard", "変更を取り消す", current.saving || current.loadingDocument) : ""}${selected.pinned ? `<button type="button" class="research-button" data-research-record="${escapeHtml(value.id)}">現在版を開く</button>` : ""}</div></header>
  ${selected.pinned ? '<p class="research-notice">履歴に固定された版を表示しています。根拠も当時採用した改訂へ接続します。</p>' : ""}
  ${dirty ? '<p class="research-draft-status" role="status">未保存の変更があります。根拠の参照を保持し、保存時に他の更新との競合を確認します。</p>' : ""}
  ${value.previewWarning ? `<p class="research-notice">${escapeHtml(value.previewWarning)}</p>` : ""}
  ${wiki.warnings.length ? `<div class="research-notice research-warnings"><strong>点検・再検討が必要な箇所</strong><ul>${wiki.warnings.map((warning) => `<li>${warning.sectionId ? `${escapeHtml(wiki.sections.find((section) => section.id === warning.sectionId)?.title ?? "節")}：` : ""}${escapeHtml(warning.reason)}</li>`).join("")}</ul></div>` : ""}
  <div class="research-wiki-dates"><span>探索の基準日：${escapeHtml(wiki.searchThrough || "未記録")}</span><span>根拠の最終点検：${escapeHtml(wiki.verifiedAt || "未記録")}</span></div>
  ${current.history ? `<section class="research-wiki-history"><h3>ページの変更履歴</h3>${wiki.history.map((entry) => `<button type="button" data-research-record="${escapeHtml(value.id)}" data-research-revision="${entry.revision}"><strong>${escapeHtml(entry.createdAt)}${entry.revision === value.revision ? " · 表示中" : ""}</strong><span>${escapeHtml(entry.changeReason || "変更理由の記録なし")}</span><small>${escapeHtml(entry.author)}</small></button>`).join("")}</section>` : editing && draft.wiki ? `<p class="research-edit-help">節ごとに本文を編集できます。保存した人間の変更は、独立点検済みとして引き継ぎません。</p>${wikiEditorTemplate(draft.wiki)}` : `<article class="research-markdown" id="research-markdown">${renderResearchMarkdown(dirty ? displayDraft?.summary ?? wiki.summary : wiki.previewSummary ?? wiki.summary)}${sections.length ? `<nav class="research-wiki-toc" aria-label="ページ内の節">${sections.map((section) => `<button type="button" data-wiki-scroll="${escapeHtml(section.id)}">${escapeHtml(section.title)}</button>`).join("")}</nav>` : '<p class="research-notice">本文は作成待ちです。根拠を確認した内容から節を追加してください。</p>'}${sections.map((section) => {
    const original = wiki.sections.find((item) => item.id === section.id);
    return `<section id="wiki-section-${escapeHtml(section.id)}" class="research-wiki-section${current.selectedSection === section.id ? " selected" : ""}"><h2>${escapeHtml(section.title)}</h2><div class="research-wiki-section-status"><span>${escapeHtml(interpretationLabel(original?.interpretation))} · ${escapeHtml(reviewLabel(original?.reviewStatus))}</span><button type="button" data-wiki-request-section="${escapeHtml(section.id)}">この節を調査対象にする</button></div>${renderResearchMarkdown(dirty ? section.markdown : original?.previewMarkdown ?? section.markdown)}${original?.audit?.length ? `<details class="research-wiki-audit"><summary>点検記録を確認</summary>${original.audit.map((audit) => `<p>${escapeHtml(audit.reviewer)} · ${escapeHtml(audit.reviewed_at)} · 対象版 ${audit.reviewed_revision}<br>${escapeHtml(audit.findings)}</p>`).join("")}</details>` : ""}${original?.basis.length ? `<aside class="research-references"><h3>この節が採用した根拠</h3>${original.basis.map((reference, index) => `<button type="button" ${reference.previewPath ? `data-research-document="${escapeHtml(reference.previewPath)}"` : `data-research-record="${escapeHtml(reference.id)}" data-research-revision="${reference.revision}"`}${reference.sectionId ? ` data-research-section="${escapeHtml(reference.sectionId)}"` : ""}${current.saving ? " disabled" : ""}>${escapeHtml(relationLabel(reference.relation))} · ${escapeHtml(reference.title ?? `根拠 ${index + 1}`)} <span>採用版を開く</span></button>`).join("")}</aside>` : '<p class="research-wiki-unreviewed">この節には固定した根拠がまだありません。</p>'}</section>`;
  }).join("")}</article>`}
  <footer class="research-wiki-related">${wiki.navigation.length ? `<h3>続きを読む・関連項目</h3><div class="research-wiki-links">${wiki.navigation.map((link) => navigate(link.id, link.title)).join("")}</div>` : ""}${wiki.backlinks.length ? `<h3>この項目を参照するページ</h3><div class="research-wiki-links">${wiki.backlinks.map((link) => navigate(link.id, link.title)).join("")}</div>` : ""}</footer>`;
}

export async function refreshResearch(): Promise<void> {
  const current = state;
  if (!current || current.loading || current.demo) return;
  const selected = current.saving || current.loadingDocument ? null : current.selected;
  const documentGeneration = current.documentGeneration;
  current.loading = true;
  current.error = "";
  render();
  const requestChanges = current.requestChanges;
  try {
    const [status, request] = await Promise.allSettled([
      invoke<WorkspaceStatus>("research_workspace_status", { workspaceRoot: current.root }),
      current.requestLoaded ? Promise.resolve(null) : invoke<{ text: string } | null>("read_research_request", { workspaceRoot: current.root }),
    ]);
    if (status.status === "fulfilled") current.status = status.value;
    else current.error = String(status.reason);
    if (request.status === "fulfilled") {
      if (request.value && requestChanges === current.requestChanges) { current.request = request.value.text; current.requestSaved = true; }
      current.requestLoaded = true;
    } else current.error = [current.error, `保存済みの依頼を読み込めませんでした：${String(request.reason)}`].filter(Boolean).join("\n");
    current.loaded = true;
    if (isCurrent(current)) {
      if (current.status?.runtime.ready) await loadWiki(current);
      const refreshes = [];
      if (current.status?.runtime.ready && current.wikiStatus && !current.wikiStatus.needsMigration) refreshes.push(loadRecords(current, false));
      if (selected && current.selected === selected && current.documentGeneration === documentGeneration) refreshes.push(refreshSelected(current, selected));
      await Promise.all(refreshes);
    }
  } catch (error) { current.error = String(error); }
  finally { current.loading = false; if (isCurrent(current)) render(); }
}

function wikiRequest<T>(current: ResearchState, request: Record<string, unknown>): Promise<T> {
  return invoke<T>("research_wiki_request", { workspaceRoot: current.root, request: { version: 1, ...request } });
}

async function loadWiki(current: ResearchState): Promise<void> {
  if (current.demo || !current.status?.runtime.ready) return;
  const generation = ++current.wikiGeneration;
  const query = current.tab === "wiki" ? current.query : "";
  current.wikiLoading = true;
  if (isCurrent(current)) render();
  try {
    const status = await wikiRequest<{ formatVersion: number; needsMigration: boolean }>(current, { operation: "wiki-status" });
    if (generation !== current.wikiGeneration) return;
    current.wikiStatus = status;
    if (status.needsMigration) return;
    let home = await wikiRequest<WikiHome>(current, { operation: "wiki-home", query, limit: 200 });
    while (home.hasMore && home.nextOffset !== null && home.nextOffset !== undefined) {
      if (generation !== current.wikiGeneration) return;
      const next = await wikiRequest<WikiHome>(current, { operation: "wiki-home", query, limit: 200, offset: home.nextOffset });
      home = { ...next, pages: [...home.pages, ...next.pages] };
    }
    if (generation === current.wikiGeneration) current.wiki = home;
  } catch (error) { if (generation === current.wikiGeneration) current.error = `研究wikiを読み込めませんでした：${String(error)}`; }
  finally { if (generation === current.wikiGeneration) { current.wikiLoading = false; if (isCurrent(current)) render(); } }
}

async function migrateWiki(current: ResearchState): Promise<void> {
  if (current.wikiLoading || current.saving || current.demo) return;
  current.wikiLoading = true;
  current.error = "";
  render();
  let migrated = false;
  try {
    const result = await wikiRequest<{ backupPath: string }>(current, { operation: "migrate" });
    current.message = `研究記録を保持して保存形式を更新しました。バックアップ：${result.backupPath}`;
    migrated = true;
  } catch (error) { current.error = `移行できませんでした：${String(error)}`; }
  finally { current.wikiLoading = false; if (isCurrent(current)) { if (migrated) await refreshResearch(); else render(); } }
}

async function createWiki(current: ResearchState): Promise<void> {
  if (!current.newWiki || current.creatingWiki || current.demo) return;
  if (!current.newWiki.title.trim()) { current.error = "項目名を入力してください。"; render(); return; }
  const draft = current.newWiki;
  const generation = ++current.documentGeneration;
  current.loadingDocument = false;
  current.creatingWiki = true;
  current.error = "";
  render();
  try {
    const value = await wikiRequest<RecordDetail>(current, { operation: "create-wiki", title: draft.title.trim(), pageType: draft.pageType, ...(draft.parent ? { parentId: draft.parent.id } : {}) });
    if (current.newWiki === draft) current.newWiki = null;
    if (generation === current.documentGeneration) {
      current.selected = { type: "record", value, pinned: false }; current.requestSaved = false;
      current.selectedSection = null;
      current.history = false;
      current.wikiView = "pages";
      current.tab = "wiki";
      draftFor(current, current.selected).editing = true;
      current.message = "項目を作成しました。本文を節ごとに追加できます。";
    } else current.message = `「${value.title}」を作成しました。研究wikiの一覧から開けます。`;
    await loadWiki(current);
  } catch (error) { current.error = `項目を作成できませんでした：${String(error)}`; }
  finally { current.creatingWiki = false; if (isCurrent(current)) render(); }
}

async function refreshSelected(current: ResearchState, selected: Selection): Promise<void> {
  if (current.saving || current.loadingDocument) return;
  const generation = ++current.documentGeneration;
  current.loadingDocument = true;
  render();
  try {
    const updated: Selection = selected.type === "record"
      ? { type: "record", pinned: selected.pinned, value: await invoke<RecordDetail>("research_record", { workspaceRoot: current.root, id: selected.value.id, revision: selected.pinned ? selected.value.revision : null }) }
      : { type: "file", value: await invoke<FileDetail>("read_research_document", { workspaceRoot: current.root, path: selected.value.path }) };
    if (generation !== current.documentGeneration || current.selected !== selected) return;
    const draft = current.drafts.get(keyFor(selected));
    if (draft && draft.text !== draft.original && keyFor(selected) !== keyFor(updated)) {
      current.message = "最新版を表示しました。編集中の旧改訂は「未保存の下書き」から再び開けます。";
    }
    current.selected = updated;
    current.requestSaved = false;
    if (updated.type === "record" && updated.value.wiki && current.selectedSection && !updated.value.wiki.sections.some((section) => section.id === current.selectedSection)) current.selectedSection = null;
  } catch (error) {
    if (generation === current.documentGeneration) current.error = [current.error, `資料を更新できませんでした：${String(error)}`].filter(Boolean).join("\n");
  } finally {
    if (generation === current.documentGeneration) { current.loadingDocument = false; if (isCurrent(current)) render(); }
  }
}

async function loadRecords(current: ResearchState, append: boolean): Promise<void> {
  if (current.demo || !current.status?.runtime.ready) return;
  const criteria = append && current.pageCriteria ? current.pageCriteria : { query: current.query, kind: current.kind };
  const generation = ++current.queryGeneration;
  current.loadingRecords = true;
  if (isCurrent(current)) render();
  try {
    const page = await invoke<RecordPage>("research_records", { workspaceRoot: current.root, query: criteria.query, kind: criteria.kind || null, offset: append ? current.page?.nextOffset ?? 0 : 0 });
    if (generation !== current.queryGeneration) return;
    current.page = append && current.page ? { ...page, records: [...current.page.records, ...page.records] } : page;
    current.pageCriteria = criteria;
  } catch (error) { if (generation === current.queryGeneration) current.error = String(error); }
  finally { if (generation === current.queryGeneration) { current.loadingRecords = false; if (isCurrent(current)) render(); } }
}

async function openRecord(current: ResearchState, id: string, revision?: number, sectionId?: string): Promise<void> {
  if (current.demo) return;
  if (current.saving) { current.message = "変更を保存中です。完了後に資料を開いてください。"; if (isCurrent(current)) render(); return; }
  const generation = ++current.documentGeneration;
  current.loadingDocument = true;
  current.error = "";
  render();
  try {
    const value = await invoke<RecordDetail>("research_record", { workspaceRoot: current.root, id, revision: revision ?? null });
    if (generation === current.documentGeneration) {
      if (current.selected?.type === "record" && current.selected.value.wiki && current.selected.value.id !== id) current.wikiReturn = { id: current.selected.value.id, revision: current.selected.value.revision, sectionId: current.selectedSection ?? undefined };
      current.selected = { type: "record", value, pinned: revision !== undefined };
      current.requestSaved = false;
      current.selectedSection = sectionId ?? null;
      current.history = false;
      current.newWiki = null;
      current.wikiView = "pages";
      if (value.wiki) current.tab = "wiki";
    }
  } catch (error) { if (generation === current.documentGeneration) current.error = String(error); }
  finally { if (generation === current.documentGeneration) { current.loadingDocument = false; if (isCurrent(current)) { render(); if (sectionId) scrollToWikiSection(sectionId); } } }
}

async function openDocument(current: ResearchState, path: string, fragment?: string): Promise<void> {
  if (current.demo) return;
  if (current.saving) { current.message = "変更を保存中です。完了後に資料を開いてください。"; if (isCurrent(current)) render(); return; }
  const generation = ++current.documentGeneration;
  current.loadingDocument = true;
  current.error = "";
  render();
  try {
    const value = await invoke<FileDetail>("read_research_document", { workspaceRoot: current.root, path });
    if (generation === current.documentGeneration) {
      if (current.selected?.type === "record" && current.selected.value.wiki) current.wikiReturn = { id: current.selected.value.id, revision: current.selected.value.revision, sectionId: current.selectedSection ?? undefined };
      current.selected = { type: "file", value }; current.selectedSection = null; current.newWiki = null; current.wikiView = "pages"; current.requestSaved = false;
    }
  } catch (error) { if (generation === current.documentGeneration) current.error = String(error); }
  finally {
    if (generation === current.documentGeneration) {
      current.loadingDocument = false;
      if (isCurrent(current)) { render(); if (fragment) scrollToFragment(fragment); }
    }
  }
}

/** Open a workspace report reached from another Bukan view. */
export async function openResearchDocument(path: string, fragment?: string): Promise<void> {
  if (!state) throw new Error("研究ワークスペースを開いてください。");
  await openDocument(state, path, fragment);
}

function scrollToFragment(fragment: string): void {
  const normalized = (text: string) => text.toLocaleLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, "").trim().replace(/\s+/g, "-");
  let anchor = fragment.replace(/^#/, "");
  try { anchor = decodeURIComponent(anchor); } catch { /* A literal percent in a heading is valid. */ }
  const section = document.getElementById(`wiki-section-${anchor}`);
  if (section) { section.scrollIntoView({ block: "start" }); return; }
  const exportedAnchor = [...document.querySelectorAll<HTMLElement>("#research-markdown [data-research-anchor]")].find((item) => item.dataset.researchAnchor === anchor);
  if (exportedAnchor) { exportedAnchor.scrollIntoView({ block: "start" }); return; }
  const heading = [...document.querySelectorAll<HTMLElement>("#research-markdown h1, #research-markdown h2, #research-markdown h3, #research-markdown h4, #research-markdown h5, #research-markdown h6")].find((item) => item.id === anchor || normalized(item.textContent ?? "") === normalized(anchor));
  heading?.scrollIntoView({ block: "start" });
}

function scrollToWikiSection(id: string): void { document.getElementById(`wiki-section-${id}`)?.scrollIntoView({ block: "start" }); }

async function followLink(current: ResearchState, selected: Selection, href: string): Promise<void> {
  if (href.startsWith("#")) { scrollToFragment(href); return; }
  const recordLink = /^bukan:record:([a-zA-Z0-9][a-zA-Z0-9_.:-]*?)(?:@(\d+))?(?:#(.+))?$/.exec(href);
  const wikiLink = /^bukan:wiki:([a-zA-Z0-9][a-zA-Z0-9_.:-]*)(?:#(.+))?$/.exec(href);
  if (recordLink?.[1]) {
    if (recordLink[2]) await openPinnedReference(current, selected, recordLink[1], Number(recordLink[2]), recordLink[3]);
    else await openRecord(current, recordLink[1], undefined, recordLink[3]);
    return;
  }
  if (wikiLink?.[1]) { await openRecord(current, wikiLink[1], undefined, wikiLink[2]); return; }
  if (/^https?:\/\//i.test(href)) { await invoke("open_research_external", { url: href }); return; }
  const documentPath = selected.type === "record" ? selected.value.editPath ?? selected.value.path : selected.value.path;
  if (!documentPath) throw new Error("この記録にはリンクの参照元がありません。書き出されたノートから確認してください。");
  const result = await invoke<{ kind: "document" | "pdf" | "external"; path: string; fragment?: string }>("resolve_research_link", { workspaceRoot: current.root, documentPath, href });
  if (!isCurrent(current) || current.selected !== selected) return;
  if (result.kind === "document") await openDocument(current, result.path, result.fragment);
  else if (result.kind === "pdf") {
    const match = /(?:^|[#&])page=(\d+)/.exec(result.fragment ?? "");
    callbacks?.openPaper(result.path, match?.[1] ? Number(match[1]) : undefined);
  } else await invoke("open_research_external", { url: result.path });
}

async function openPinnedReference(current: ResearchState, selected: Selection, id: string, revision: number, fragment?: string): Promise<void> {
  if (selected.type === "record" && selected.value.wiki && id !== selected.value.id) {
    if (selected.value.previewWarning) {
      current.error = "保存当時の資料を開けません。wikiの書き出しに関するお知らせを確認してください。";
      if (isCurrent(current)) render();
      return;
    }
    const matches = (selected.value.wiki.frozenReferences ?? []).filter((reference) => reference.id === id && reference.revision === revision);
    const ownContext = matches.filter((reference) => reference.contextId === selected.value.id && reference.contextRevision === selected.value.revision);
    const references = ownContext.length ? ownContext : matches;
    if (references.length === 1 && references[0]) { await openDocument(current, references[0].previewPath, fragment); return; }
    if (references.length > 1) {
      current.frozenChoices = { source: selected, references, fragment };
      if (isCurrent(current)) render();
      return;
    }
  }
  await openRecord(current, id, revision, fragment);
}

async function saveSelected(current: ResearchState): Promise<void> {
  const selected = current.selected;
  if (!selected || !selected.value.editable || selected.type === "record" && selected.pinned || current.saving || current.loadingDocument || current.demo) return;
  const draft = draftFor(current, selected);
  const text = draft.text;
  current.saving = true;
  current.error = "";
  render();
  try {
    const updated: Selection = selected.type === "record" ? { type: "record", pinned: false, value: draft.wiki ? await wikiRequest<RecordDetail>(current, { operation: "save-wiki", id: selected.value.id, expectedRevision: selected.value.revision, ...draft.wiki }) : await invoke<RecordDetail>("save_research_note", { workspaceRoot: current.root, id: selected.value.id, expectedRevision: selected.value.revision, markdown: text }) } : { type: "file", value: await invoke<FileDetail>("save_research_document", { workspaceRoot: current.root, path: selected.value.path, expectedSha256: draft.baseSha256 ?? selected.value.sha256, markdown: text }) };
    const savedWiki = updated.type === "record" ? wikiEditFor(updated.value) : undefined;
    const unchangedWhileSaving = draft.text === text;
    const nextWiki = unchangedWhileSaving ? savedWiki : draft.wiki;
    const nextOriginal = savedWiki ? JSON.stringify(savedWiki) : text;
    current.drafts.delete(keyFor(selected));
    current.drafts.set(keyFor(updated), { text: unchangedWhileSaving ? nextOriginal : draft.text, original: nextOriginal, wiki: nextWiki, editing: draft.editing, source: updated, baseSha256: updated.type === "file" ? updated.value.sha256 : undefined });
    if (current.selected === selected) current.selected = updated;
    current.requestSaved = false;
    current.message = draft.wiki ? "研究wikiを新しい改訂として保存しました。変更した節は点検待ちです。" : selected.type === "record" ? "文献ノートを新しい改訂として保存しました。" : "ファイルの変更を保存しました。";
    if (draft.wiki && isCurrent(current)) await loadWiki(current);
    if (isCurrent(current) && current.status?.runtime.ready) await loadRecords(current, false);
  } catch (error) { current.error = `保存できませんでした。編集中の内容は保持しています。${String(error)}`; }
  finally { current.saving = false; if (isCurrent(current)) render(); }
}

async function prepare(current: ResearchState): Promise<void> {
  if (current.preparing || current.demo) return;
  current.preparing = true;
  current.error = "";
  render();
  try {
    await invoke<ResearchRuntime>("prepare_research_environment");
    if (isCurrent(current)) await refreshResearch();
  } catch (error) { current.error = String(error); }
  finally { current.preparing = false; if (isCurrent(current)) render(); }
}

async function saveRequest(current: ResearchState): Promise<void> {
  if (current.savingRequest || current.demo) return;
  if (!current.request.trim()) { current.error = "調べたいことを入力してください。"; render(); return; }
  const selected = current.selected;
  if (selected?.type === "record" && selected.value.wiki && !selected.pinned && draftFor(current, selected).text !== draftFor(current, selected).original) { current.error = "wikiの変更を保存してから依頼を引き継いでください。保存した本文と根拠を調査対象にします。"; render(); return; }
  const sectionId = current.selectedSection;
  const sectionTitle = selected?.type === "record" ? selected.value.wiki?.sections.find((section) => section.id === sectionId)?.title : undefined;
  const requestText = current.request;
  const taskKey = JSON.stringify([requestText, selected?.type, selected?.type === "record" ? selected.value.id : selected?.value.path, selected?.type === "record" ? selected.value.revision : null, sectionId]);
  const changes = current.requestChanges;
  let saved = false;
  current.savingRequest = true;
  current.error = "";
  render();
  try {
    if (selected?.type === "record" && selected.value.wiki && current.requestTask?.key !== taskKey) {
      const task = await wikiRequest<{ id: string }>(current, { operation: "create-wiki-task", pageId: selected.value.id, pageRevision: selected.value.revision, ...(sectionId ? { sectionId } : {}), purpose: requestText });
      current.requestTask = { key: taskKey, id: task.id };
    }
    await invoke("save_research_request", { workspaceRoot: current.root, text: requestText, recordId: selected?.type === "record" ? selected.value.id : null, recordRevision: selected?.type === "record" ? selected.value.revision : null, documentPath: selected?.value.path ?? null, sectionId, sectionTitle: sectionTitle ?? null, taskId: current.requestTask?.key === taskKey ? current.requestTask.id : null });
    saved = true;
    current.requestSaved = changes === current.requestChanges && current.selected === selected && current.selectedSection === sectionId;
    current.message = "依頼を保存しました。Codexに下の指示を入力して作業を開始してください。";
    if (isCurrent(current)) await callbacks?.openCodex();
  } catch (error) { current.error = `${saved ? "依頼は保存済みです。Codexを開けませんでした。" : current.requestTask?.key === taskKey ? "更新作業は保存済みです。対話への引き継ぎに失敗しました。再度保存すると同じ作業を引き継ぎます。" : "依頼を保存できませんでした。入力内容は保持しています。"}${String(error)}`; }
  finally { current.savingRequest = false; if (isCurrent(current)) render(); }
}

export function bindResearchEvents(nextCallbacks: ResearchCallbacks): void {
  callbacks = nextCallbacks;
  const home = document.querySelector<HTMLElement>(".research-home");
  const current = state;
  if (!home || !current) return;
  const selected = current.selected;
  home.scrollTop = current.scrollTop;
  const results = home.querySelector<HTMLElement>(".research-catalog-results");
  if (results) results.scrollTop = current.catalogScrollTop;
  if (current.focus) {
    const focused = document.getElementById(current.focus.id);
    if (focused instanceof HTMLInputElement || focused instanceof HTMLTextAreaElement) {
      focused.focus({ preventScroll: true });
      if (current.focus.start !== null && current.focus.end !== null) focused.setSelectionRange(current.focus.start, current.focus.end);
      focused.scrollTop = current.focus.scrollTop;
    }
  }
  home.querySelector<HTMLTextAreaElement>("#research-request-text")?.addEventListener("input", (event) => {
    if (event.target instanceof HTMLTextAreaElement) {
      const wasSaved = current.requestSaved;
      current.request = event.target.value;
      current.requestChanges += 1;
      current.requestSaved = false;
      if (wasSaved) {
        current.message = "依頼に未保存の変更があります。変更した依頼を渡すには、もう一度保存してください。";
        if (!(event instanceof InputEvent && event.isComposing)) render();
      }
    }
  });
  home.querySelector<HTMLTextAreaElement>("#research-request-text")?.addEventListener("compositionend", (event) => {
    if (event.target instanceof HTMLTextAreaElement) current.request = event.target.value;
    render();
  });
  home.querySelector<HTMLFormElement>("#research-request-form")?.addEventListener("submit", (event) => { event.preventDefault(); void saveRequest(current); });
  home.querySelector<HTMLInputElement>("#research-query")?.addEventListener("input", (event) => { if (event.target instanceof HTMLInputElement) current.query = event.target.value; });
  home.querySelector<HTMLFormElement>("#research-search-form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    if (current.tab === "records") void loadRecords(current, false);
    else if (current.tab === "wiki") void loadWiki(current);
    else render();
  });
  home.querySelector<HTMLSelectElement>("#research-kind")?.addEventListener("change", (event) => {
    if (event.target instanceof HTMLSelectElement) { current.kind = event.target.value; void loadRecords(current, false); }
  });
  home.querySelector<HTMLSelectElement>("#research-wiki-type")?.addEventListener("change", (event) => { if (event.target instanceof HTMLSelectElement) { current.wikiType = event.target.value; render(); } });
  home.querySelector<HTMLSelectElement>("#research-wiki-category")?.addEventListener("change", (event) => { if (event.target instanceof HTMLSelectElement) { current.wikiCategory = event.target.value; render(); } });
  home.querySelector<HTMLFormElement>("#research-wiki-create-form")?.addEventListener("submit", (event) => { event.preventDefault(); void createWiki(current); });
  home.querySelector<HTMLInputElement>("#research-wiki-new-title")?.addEventListener("input", (event) => { if (current.newWiki && event.target instanceof HTMLInputElement) current.newWiki.title = event.target.value; });
  home.querySelector<HTMLSelectElement>("#research-wiki-new-type")?.addEventListener("change", (event) => { if (current.newWiki && event.target instanceof HTMLSelectElement) current.newWiki.pageType = event.target.value; });
  home.querySelector<HTMLSelectElement>("#research-wiki-new-parent")?.addEventListener("change", (event) => {
    if (current.newWiki && event.target instanceof HTMLSelectElement) {
      const value = event.target.value;
      const option = [...event.target.options].find((item) => item.value === value);
      current.newWiki.parent = value && option ? { id: value, title: option.textContent ?? "" } : null;
    }
  });
  home.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>("[data-wiki-field]").forEach((input) => {
    const updateDraft = () => {
      if (!selected) return false;
      const draft = draftFor(current, selected);
      if (!draft.wiki) return false;
      const previouslyDirty = draft.text !== draft.original;
      const field = input.dataset.wikiField;
      if (input.dataset.wikiSectionIndex !== undefined) {
        const section = draft.wiki.sections[Number(input.dataset.wikiSectionIndex)];
        if (section && (field === "title" || field === "markdown")) section[field] = input.value;
      } else if (field === "title" || field === "summary" || field === "changeReason") draft.wiki[field] = input.value;
      draft.text = JSON.stringify(draft.wiki);
      return previouslyDirty !== (draft.text !== draft.original);
    };
    input.addEventListener("input", (event) => { if (updateDraft() && !(event instanceof InputEvent && event.isComposing)) render(); });
    input.addEventListener("compositionend", () => { updateDraft(); render(); });
  });
  home.querySelector<HTMLTextAreaElement>("#research-note-editor")?.addEventListener("input", (event) => {
    if (selected && event.target instanceof HTMLTextAreaElement) {
      const draft = draftFor(current, selected);
      const previouslyDirty = draft.text !== draft.original;
      draft.text = event.target.value;
      if (previouslyDirty !== (draft.text !== draft.original) && !(event instanceof InputEvent && event.isComposing)) {
        const start = event.target.selectionStart;
        const end = event.target.selectionEnd;
        const scrollTop = event.target.scrollTop;
        render();
        const editor = document.querySelector<HTMLTextAreaElement>("#research-note-editor");
        editor?.focus(); editor?.setSelectionRange(start, end); if (editor) editor.scrollTop = scrollTop;
      }
    }
  });
  home.querySelector<HTMLTextAreaElement>("#research-note-editor")?.addEventListener("compositionend", (event) => {
    if (selected && event.target instanceof HTMLTextAreaElement) draftFor(current, selected).text = event.target.value;
    render();
  });
  home.addEventListener("click", (event) => {
    if (!(event.target instanceof Element)) return;
    const target = event.target.closest<HTMLButtonElement>("button");
    if (!target || target.disabled) return;
    if (target.dataset.researchRecord) {
      const revision = target.dataset.researchRevision ? Number(target.dataset.researchRevision) : undefined;
      if (revision !== undefined && selected) void openPinnedReference(current, selected, target.dataset.researchRecord, revision, target.dataset.researchSection);
      else void openRecord(current, target.dataset.researchRecord, revision, target.dataset.researchSection);
    }
    else if (target.dataset.researchDocument) void openDocument(current, target.dataset.researchDocument);
    else if (target.dataset.researchFrozen !== undefined && current.frozenChoices?.source === current.selected) {
      const choice = current.frozenChoices;
      const reference = choice.references[Number(target.dataset.researchFrozen)];
      if (reference) { current.frozenChoices = null; void openDocument(current, reference.previewPath, choice.fragment); }
    }
    else if (target.dataset.wikiScroll) scrollToWikiSection(target.dataset.wikiScroll);
    else if (target.dataset.wikiRequestSection) {
      current.selectedSection = target.dataset.wikiRequestSection;
      current.requestSaved = false;
      render();

      document.getElementById("research-request-text")?.focus();
    }
    else if (target.dataset.researchDraft) {
      const draft = current.drafts.get(target.dataset.researchDraft);
      if (draft) { current.documentGeneration += 1; current.loadingDocument = false; current.selected = draft.source.type === "record" ? { ...draft.source, pinned: false } : draft.source; current.history = false; current.selectedSection = null; current.requestSaved = false; current.newWiki = null; current.wikiView = "pages"; draft.editing = true; current.message = "未保存の下書きを開きました。保存時に他の更新との競合を確認します。"; render(); }
    }
    else if (target.dataset.researchKind) { current.kind = target.dataset.researchKind; current.tab = "records"; current.query = ""; void loadRecords(current, false); }
    else if (target.dataset.researchTab === "wiki" || target.dataset.researchTab === "records" || target.dataset.researchTab === "documents") {
      current.tab = target.dataset.researchTab;
      current.wikiView = "pages";
      if (current.tab === "wiki" && !(current.selected?.type === "record" && current.selected.value.wiki)) { current.documentGeneration += 1; current.loadingDocument = false; current.selected = null; current.requestSaved = false; }
      if (current.tab === "records" && (current.pageCriteria?.query !== current.query || current.pageCriteria?.kind !== current.kind)) void loadRecords(current, false);
      else if (current.tab === "wiki") void loadWiki(current);
      else render();
    }
    else {
      const action = target.dataset.researchAction;
      if (action === "refresh") void refreshResearch();
      if (action === "more") void loadRecords(current, true);
      if (action === "prepare") void prepare(current);
      if (action === "save") void saveSelected(current);
      if (action === "edit" && current.selected) { const draft = draftFor(current, current.selected); draft.editing = !draft.editing; current.history = false; render(); }
      if (action === "discard" && current.selected && window.confirm("この資料の未保存の変更を取り消しますか？")) { const draft = draftFor(current, current.selected); draft.text = draft.original; draft.wiki = current.selected.type === "record" ? wikiEditFor(current.selected.value) : undefined; if (current.selectedSection && !draft.wiki?.sections.some((section) => section.id === current.selectedSection)) { current.selectedSection = null; current.requestSaved = false; } render(); }
      if (action === "wiki-migrate") void migrateWiki(current);

      if (action === "wiki-home" || action === "wiki-updates") {
        if (current.saving) return;
        current.documentGeneration += 1; current.loadingDocument = false;
        current.tab = "wiki"; current.selected = null; current.selectedSection = null; current.requestSaved = false; current.newWiki = null; current.query = "";
        current.wikiType = ""; current.wikiCategory = ""; current.wikiView = action === "wiki-updates" ? "updates" : "pages";
        render(); void loadWiki(current);
      }
      if (action === "wiki-create") {
        if (current.saving || current.wikiStatus?.needsMigration) return;
        current.documentGeneration += 1; current.loadingDocument = false;
        current.newWiki = { title: "", pageType: "concept", parent: current.selected?.type === "record" && current.selected.value.wiki ? { id: current.selected.value.id, title: current.selected.value.title } : null };
        current.tab = "wiki"; render(); document.getElementById("research-wiki-new-title")?.focus();
      }
      if (action === "wiki-create-cancel") { current.newWiki = null; render(); }
      if (action === "wiki-history") { current.history = true; render(); }
      if (action === "wiki-read") { current.history = false; if (current.selected) draftFor(current, current.selected).editing = false; render(); }
      if (action === "wiki-return" && current.wikiReturn) void openRecord(current, current.wikiReturn.id, current.wikiReturn.revision, current.wikiReturn.sectionId);
      if (action === "wiki-add-section" && current.selected) {
        const draft = draftFor(current, current.selected);
        if (draft.wiki) { draft.wiki.sections.push({ id: `section-${crypto.randomUUID()}`, title: "新しい節", markdown: "" }); draft.text = JSON.stringify(draft.wiki); render(); }
      }
      if (action === "copy-request") void navigator.clipboard.writeText(".bukan/current-research.md を読んで、依頼を進めて").then(() => { current.message = "Codexに入力する指示をコピーしました。"; if (isCurrent(current)) render(); }).catch((error: unknown) => { current.error = `コピーできませんでした。表示されている指示を選択してコピーしてください。${String(error)}`; if (isCurrent(current)) render(); });
    }
  });
  const markdown = home.querySelector<HTMLElement>("#research-markdown");
  if (markdown && selected) hydrateResearchMarkdown(markdown, {
    workspaceRoot: current.root, documentPath: previewPath(current, selected),
    onLink: (href) => followLink(current, selected, href),
    onError: (message) => { if (isCurrent(current)) { current.error = message; render(); } },
  });
  if (!current.loaded && !current.loading) void refreshResearch();
}

export function setResearchDemo(): void {
  setResearchWorkspace("デモ / ダスト輸送", "月面ダスト輸送の研究");
  if (!state) return;
  state.demo = true;
  state.loaded = true;
  state.status = { workspaceRoot: state.root, name: state.name ?? "", storePath: "デモ", runtime: { available: true, ready: true, detail: "デモ表示" }, documents: [{ path: "reports/research-history.md", title: "ダスト輸送の研究史", group: "研究史・レポート", modifiedAt: 0 }] };
  state.page = { info: { counts: { paper: 12, paper_note: 12, question: 8, claim: 78, relation: 22, review_task: 24 } }, records: [{ id: "question-detachment", revision: 1, kind: "question", title: "粒子はどの条件で表面から離脱するか", summary: "照射条件、付着力、粒径と接触状態を分け、実証された範囲と未検証の仮定を追う。", updatedAt: "", state: "open" }, { id: "relation-2016-2025", revision: 1, kind: "relation", title: "電子放出と複合照射の実験を比較する", summary: "異なる測定条件で得られた結果を、そのまま同じ機構の証拠として扱えるか。", updatedAt: "" }], hasMore: false, nextOffset: 2 };
  state.selected = { type: "record", pinned: false, value: { id: "question-detachment", revision: 1, kind: "question", title: "粒子はどの条件で表面から離脱するか", markdown: "## 調べたいこと\n\n粒子の離脱・浮上・輸送を区別し、条件ごとに先行研究の主張と根拠を対応させる。\n\n## 比較する条件\n\n| 条件 | 確認する内容 |\n|---|---|\n| 照射 | UV・電子ビーム・複合照射の差 |\n| 粒子 | 粒径・材質・接触状態 |\n| 測定 | 直接観測とモデルからの推定 |\n\n$$\nF_{\\mathrm{net}} = qE - mg - F_{\\mathrm{adh}}\n$$\n\n> この画面は操作イメージ用のデモです。研究の結論を示すものではありません。", editable: false, references: [] } };
  const example = state.selected.value;
  example.kind = "wiki_page";
  example.editable = true;
  example.wiki = { summary: "粒子の離脱・浮上・輸送を区別し、条件ごとに先行研究の主張と根拠を対応させる。これは画面確認用のデモです。", pageType: "concept", parentIds: [], aliases: ["表面からの離脱", "Dust detachment"], categories: ["ダスト輸送"], sections: [{ id: "comparison", title: "比較する条件と残る問い", markdown: example.markdown, basis: [], reviewStatus: "unverified", interpretation: "human_proposal" }], navigation: [], backlinks: [], history: [{ revision: 1, createdAt: "デモ", author: "デモ", changeReason: "操作イメージ用の項目" }], warnings: [{ sectionId: "comparison", reason: "デモには検証済みの根拠を登録していません。" }] };
  state.wikiStatus = { formatVersion: 2, needsMigration: false };
  state.wiki = { pages: [{ id: example.id, revision: 1, title: example.title, summary: example.wiki.summary, pageType: "concept", parentIds: [], categories: example.wiki.categories, aliases: example.wiki.aliases, updatedAt: "" }], tasks: [], candidates: [] };
  if (state.page.info.counts) state.page.info.counts.wiki_page = 1;
}
