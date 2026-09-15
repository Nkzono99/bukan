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
}
interface FileDetail { path: string; relativePath: string; title: string; markdown: string; sha256: string; editable: boolean }
type Selection = { type: "record"; value: RecordDetail; pinned: boolean } | { type: "file"; value: FileDetail };
interface Draft { text: string; original: string; editing: boolean; source: Selection; baseSha256?: string }
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
  tab: "records" | "documents";
  selected: Selection | null;
  drafts: Map<string, Draft>;
  request: string;
  requestLoaded: boolean;
  requestChanges: number;
  requestSaved: boolean;
  queryGeneration: number;
  documentGeneration: number;
  scrollTop: number;
  catalogScrollTop: number;
  focus: { id: string; start: number | null; end: number | null; scrollTop: number } | null;
  demo: boolean;
}

const kinds = [
  ["", "すべて"], ["question", "問い・課題"], ["topic", "研究テーマ"], ["paper_note", "文献ノート"],
  ["paper", "論文"], ["claim", "主張"], ["relation", "関係・比較"], ["review_task", "レビュー作業"],
  ["evidence", "根拠"], ["source", "原文記録"],
];
const workspaceStates = new Map<string, ResearchState>();
let state: ResearchState | null = null;
let callbacks: ResearchCallbacks | null = null;

function newState(root: string, name: string | null): ResearchState {
  return {
    root, name, status: null, page: null, pageCriteria: null, loaded: false, loading: false, loadingRecords: false,
    loadingDocument: false, preparing: false, saving: false, savingRequest: false,
    error: "", message: "", query: "", kind: "", tab: "records", selected: null, drafts: new Map(),
    request: "", requestLoaded: false, requestChanges: 0, requestSaved: false,
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
function keyFor(selected: Selection): string {
  return selected.type === "record" ? `record:${selected.value.id}@${selected.value.revision}` : `file:${selected.value.path}`;
}
function draftFor(current: ResearchState, selected: Selection): Draft {
  const key = keyFor(selected);
  let draft = current.drafts.get(key);
  if (!draft) {
    const text = selected.type === "record" ? selected.value.editText ?? selected.value.markdown : selected.value.markdown;
    draft = { text, original: text, editing: false, source: selected, baseSha256: selected.type === "file" ? selected.value.sha256 : undefined };
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
  return selected.type === "record" && draft.text !== draft.original ? selected.value.editPath : selected.value.path;
}
function button(action: string, label: string, disabled = false, primary = false): string {
  return `<button type="button" class="research-button${primary ? " primary" : ""}" data-research-action="${action}"${disabled ? " disabled" : ""}>${label}</button>`;
}

export function researchTemplate(): string {
  const current = state;
  if (!current) return `<section class="research-home"><div class="research-empty"><h1>研究ホーム</h1><p>研究ワークスペースを開くと、問い・文献ノート・解析結果をここから扱えます。</p><p>サイドバーからワークスペースを開くか、新しく作成してください。</p></div></section>`;
  const previous = document.querySelector<HTMLElement>(".research-home");
  if (previous?.dataset.researchWorkspace === current.root) {
    current.scrollTop = previous.scrollTop;
    current.catalogScrollTop = previous.querySelector(".research-catalog-results")?.scrollTop ?? 0;
    const focused = document.activeElement;
    current.focus = focused && previous.contains(focused) && (focused instanceof HTMLTextAreaElement || focused instanceof HTMLInputElement) ? { id: focused.id, start: focused.selectionStart, end: focused.selectionEnd, scrollTop: focused.scrollTop } : null;
  }
  const runtime = current.status?.runtime;
  const counts = current.page?.info.counts ?? {};
  return `<section class="research-home" aria-label="研究ホーム" data-research-workspace="${escapeHtml(current.root)}">
    <header class="research-header"><div><p class="section-kicker">RESEARCH WORKSPACE${current.demo ? " · DEMO" : ""}</p><h1>${escapeHtml(current.status?.name ?? current.name ?? "研究ワークスペース")}</h1><p class="research-path" title="${escapeHtml(current.root)}">${escapeHtml(current.root)}</p></div>${button("refresh", current.loading ? "更新中…" : "更新", current.loading || current.saving || current.loadingDocument || current.demo)}</header>
    ${current.error ? `<div class="research-notice error" role="alert">${escapeHtml(current.error)}</div>` : ""}
    ${current.message ? `<div class="research-notice" role="status">${escapeHtml(current.message)}</div>` : ""}
    ${current.status?.warnings?.length ? `<details class="research-notice research-warnings"><summary>資料の読み込みに関するお知らせ（${current.status.warnings.length}件）</summary><ul>${current.status.warnings.map((warning) => `<li>${escapeHtml(warning)}</li>`).join("")}</ul></details>` : ""}
    ${runtime && !runtime.ready ? `<div class="research-setup"><div><strong>研究機能の準備</strong><p>${escapeHtml(runtime.detail)}</p><small>初回はインターネット接続が必要です。保存済みのレポートは準備前でも開けます。</small></div>${button("prepare", current.preparing ? "準備中…" : "研究機能を準備", current.preparing || current.demo, true)}</div>` : ""}
    <div class="research-overview">
      <form class="research-request" id="research-request-form"><label for="research-request-text">次に調べたいこと</label><textarea id="research-request-text" rows="3" placeholder="例：ダストの離脱と輸送について、実験条件の違いと未検証の仮定を先行研究まで追って。">${escapeHtml(current.request)}</textarea><div class="research-request-actions"><span>依頼と閲覧中の資料を保存して、アプリ内のCodexに引き継ぎます。</span><button type="submit" class="research-button primary"${current.savingRequest || current.demo ? " disabled" : ""}>${current.savingRequest ? "保存中…" : "依頼を保存してCodexを開く"}</button></div>${current.requestSaved ? `<div class="research-handoff"><code>.bukan/current-research.md を読んで、依頼を進めて</code>${button("copy-request", "指示をコピー")}<small>Codexにこの指示を入力すると作業を開始できます。</small></div>` : ""}</form>
      <div class="research-counts">${[["question", "問い・課題"], ["paper", "論文"], ["paper_note", "文献ノート"], ["claim", "主張"], ["relation", "関係・比較"], ["review_task", "レビュー作業"]].map(([kind, label]) => `<button type="button" data-research-kind="${kind}"><strong>${counts[kind ?? ""] ?? "—"}</strong><span>${label}</span></button>`).join("")}<small>保存された記録数です。未処理・確認済みは各記録で確認できます。</small></div>
    </div>
    <div class="research-browser">
      <section class="research-catalog" aria-label="研究資料"><div class="research-tabs"><button type="button" data-research-tab="records" aria-pressed="${current.tab === "records"}">研究記録</button><button type="button" data-research-tab="documents" aria-pressed="${current.tab === "documents"}">ノート・レポート</button></div>
        <form class="research-search" id="research-search-form"><label class="research-sr-only" for="research-query">研究資料を検索</label><input id="research-query" type="search" value="${escapeHtml(current.query)}" placeholder="主張・課題・ノートを検索"><button type="submit" class="research-button"${current.loadingRecords ? " disabled" : ""}>検索</button>${current.tab === "records" ? `<label class="research-sr-only" for="research-kind">記録の種類</label><select id="research-kind">${kinds.map(([kind, label]) => `<option value="${kind}"${current.kind === kind ? " selected" : ""}>${label}</option>`).join("")}</select>` : ""}</form>
        ${draftsTemplate(current)}<div class="research-catalog-results" aria-busy="${current.loadingRecords}">${current.tab === "records" ? recordsTemplate(current) : documentsTemplate(current)}</div>
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
  const selected = current.selected;
  if (!selected) return `<div class="research-empty"><span class="research-empty-mark">文</span><h2>${current.loadingDocument ? "資料を読み込み中…" : "問いから、根拠まで"}</h2><p>研究記録やレポートを選ぶと、本文・数式・図表をここで読めます。</p><p>文献ノートへの追記は研究記録として保存され、次の検索と解析に使われます。</p></div>`;
  const value = selected.value;
  const draft = draftFor(current, selected);
  const dirty = draft.text !== draft.original;
  const markdown = dirty ? draft.text : value.markdown;
  const preview = selected.type === "file" && !/\.md$/i.test(selected.value.relativePath) ? `<pre><code>${escapeHtml(markdown)}</code></pre>` : renderResearchMarkdown(markdown);
  const conflict = selected.type === "file" && draft.baseSha256 !== selected.value.sha256;
  const subtitle = selected.type === "record" ? `${kindLabel(selected.value.kind)} · r${selected.value.revision} · ${selected.value.id}` : selected.value.relativePath;
  return `<header class="research-detail-header"><div><span class="research-record-meta">${escapeHtml(subtitle)}</span><h2>${escapeHtml(value.title)}</h2></div><div class="research-detail-actions">${value.editable ? button("edit", draft.editing ? "プレビュー" : "編集", current.saving || current.loadingDocument) : '<span class="research-readonly">閲覧用</span>'}${dirty ? button("save", current.saving ? "保存中…" : "変更を保存", current.saving || current.loadingDocument || current.demo, true) : ""}${dirty ? button("discard", "変更を取り消す", current.saving || current.loadingDocument) : ""}</div></header>
    ${selected.type === "record" && selected.value.previewWarning ? `<p class="research-notice">${escapeHtml(selected.value.previewWarning)}</p>` : ""}
    ${dirty ? '<p class="research-draft-status" role="status">未保存の変更があります。プレビューには編集中の内容を表示しています。</p>' : ""}
    ${conflict ? '<p class="research-notice error">このファイルは別の操作で更新されました。編集中の内容は保持しています。変更をコピーしてから取り消すと最新版を確認できます。</p>' : ""}
    ${value.editable ? `<p class="research-edit-help">${selected.type === "record" ? "保存すると文献ノートの新しい改訂が作られ、検索に反映されます。" : "このファイルを更新します。構造化した研究記録とは別に保存されます。"}</p>` : selected.type === "file" && selected.value.relativePath.replaceAll("\\", "/").startsWith("data/") ? '<p class="research-edit-help">書き出された記録は閲覧用です。追記は「研究記録」の文献ノートから保存してください。</p>' : ""}
    ${draft.editing ? `<label class="research-sr-only" for="research-note-editor">本文を編集</label><textarea class="research-note-editor" id="research-note-editor" spellcheck="false">${escapeHtml(draft.text)}</textarea>` : `<article class="research-markdown" id="research-markdown">${preview}</article>`}
    ${selected.type === "record" && selected.value.references.length ? `<footer class="research-references"><h3>根拠・関連記録</h3>${selected.value.references.map((reference) => `<button type="button" data-research-record="${escapeHtml(reference.id)}" data-research-revision="${reference.revision}"${current.saving ? " disabled" : ""}>${escapeHtml(reference.id)} <span>r${reference.revision}</span></button>`).join("")}</footer>` : ""}`;
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
      const refreshes = [];
      if (current.status?.runtime.ready) refreshes.push(loadRecords(current, false));
      if (selected && current.selected === selected && current.documentGeneration === documentGeneration) refreshes.push(refreshSelected(current, selected));
      await Promise.all(refreshes);
    }
  } catch (error) { current.error = String(error); }
  finally { current.loading = false; if (isCurrent(current)) render(); }
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

async function openRecord(current: ResearchState, id: string, revision?: number): Promise<void> {
  if (current.demo) return;
  if (current.saving) { current.message = "変更を保存中です。完了後に資料を開いてください。"; if (isCurrent(current)) render(); return; }
  const generation = ++current.documentGeneration;
  current.loadingDocument = true;
  current.error = "";
  render();
  try {
    const value = await invoke<RecordDetail>("research_record", { workspaceRoot: current.root, id, revision: revision ?? null });
    if (generation === current.documentGeneration) current.selected = { type: "record", value, pinned: revision !== undefined };
  } catch (error) { if (generation === current.documentGeneration) current.error = String(error); }
  finally { if (generation === current.documentGeneration) { current.loadingDocument = false; if (isCurrent(current)) render(); } }
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
    if (generation === current.documentGeneration) current.selected = { type: "file", value };
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
  const heading = [...document.querySelectorAll<HTMLElement>("#research-markdown h1, #research-markdown h2, #research-markdown h3, #research-markdown h4, #research-markdown h5, #research-markdown h6")].find((item) => item.id === anchor || normalized(item.textContent ?? "") === normalized(anchor));
  heading?.scrollIntoView({ block: "start" });
}

async function followLink(current: ResearchState, selected: Selection, href: string): Promise<void> {
  if (href.startsWith("#")) { scrollToFragment(href); return; }
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

async function saveSelected(current: ResearchState): Promise<void> {
  const selected = current.selected;
  if (!selected || !selected.value.editable || current.saving || current.loadingDocument || current.demo) return;
  const draft = draftFor(current, selected);
  const text = draft.text;
  current.saving = true;
  current.error = "";
  render();
  try {
    const updated: Selection = selected.type === "record" ? { type: "record", pinned: selected.pinned, value: await invoke<RecordDetail>("save_research_note", { workspaceRoot: current.root, id: selected.value.id, expectedRevision: selected.value.revision, markdown: text }) } : { type: "file", value: await invoke<FileDetail>("save_research_document", { workspaceRoot: current.root, path: selected.value.path, expectedSha256: draft.baseSha256 ?? selected.value.sha256, markdown: text }) };
    current.drafts.delete(keyFor(selected));
    current.drafts.set(keyFor(updated), { text: draft.text, original: text, editing: draft.editing, source: updated, baseSha256: updated.type === "file" ? updated.value.sha256 : undefined });
    if (current.selected === selected) current.selected = updated;
    current.message = selected.type === "record" ? "文献ノートを新しい改訂として保存しました。" : "ファイルの変更を保存しました。";
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
  const changes = current.requestChanges;
  let saved = false;
  current.savingRequest = true;
  current.error = "";
  render();
  try {
    await invoke("save_research_request", { workspaceRoot: current.root, text: current.request, recordId: selected?.type === "record" ? selected.value.id : null, recordRevision: selected?.type === "record" ? selected.value.revision : null, documentPath: selected?.value.path ?? null });
    saved = true;
    current.requestSaved = changes === current.requestChanges;
    current.message = "依頼を保存しました。Codexに下の指示を入力して作業を開始してください。";
    if (isCurrent(current)) await callbacks?.openCodex();
  } catch (error) { current.error = `${saved ? "依頼は保存済みです。Codexを開けませんでした。" : "依頼を保存できませんでした。入力内容は保持しています。"}${String(error)}`; }
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
    else render();
  });
  home.querySelector<HTMLSelectElement>("#research-kind")?.addEventListener("change", (event) => {
    if (event.target instanceof HTMLSelectElement) { current.kind = event.target.value; void loadRecords(current, false); }
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
    if (target.dataset.researchRecord) void openRecord(current, target.dataset.researchRecord, target.dataset.researchRevision ? Number(target.dataset.researchRevision) : undefined);
    else if (target.dataset.researchDocument) void openDocument(current, target.dataset.researchDocument);
    else if (target.dataset.researchDraft) {
      const draft = current.drafts.get(target.dataset.researchDraft);
      if (draft) { current.selected = draft.source.type === "record" ? { ...draft.source, pinned: true } : draft.source; draft.editing = true; current.message = "未保存の下書きを開きました。保存時に他の更新との競合を確認します。"; render(); }
    }
    else if (target.dataset.researchKind) { current.kind = target.dataset.researchKind; current.tab = "records"; current.query = ""; void loadRecords(current, false); }
    else if (target.dataset.researchTab === "records" || target.dataset.researchTab === "documents") {
      current.tab = target.dataset.researchTab;
      if (current.tab === "records" && (current.pageCriteria?.query !== current.query || current.pageCriteria?.kind !== current.kind)) void loadRecords(current, false);
      else render();
    }
    else {
      const action = target.dataset.researchAction;
      if (action === "refresh") void refreshResearch();
      if (action === "more") void loadRecords(current, true);
      if (action === "prepare") void prepare(current);
      if (action === "save") void saveSelected(current);
      if (action === "edit" && current.selected) { const draft = draftFor(current, current.selected); draft.editing = !draft.editing; render(); }
      if (action === "discard" && current.selected && window.confirm("この資料の未保存の変更を取り消しますか？")) { const draft = draftFor(current, current.selected); draft.text = draft.original; render(); }
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
}
