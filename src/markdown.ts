import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import DOMPurify from "dompurify";
import katex from "katex";
import { Marked } from "marked";
import "katex/dist/katex.min.css";

export function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (character) => ({
    "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
  })[character] ?? character);
}

function mathHtml(text: string, displayMode: boolean): string {
  return katex.renderToString(text, { displayMode, throwOnError: false, trust: false, strict: "ignore" });
}

const markdown = new Marked({
  gfm: true,
  async: false,
  renderer: {
    html({ text }) { return escapeHtml(text); },
    checkbox({ checked }) { return `<span aria-label="${checked ? "完了" : "未完了"}">${checked ? "☑" : "☐"}</span> `; },
    link({ href, title, tokens }) {
      return `<a href="#" data-research-href="${escapeHtml(href)}"${title ? ` title="${escapeHtml(title)}"` : ""}>${this.parser.parseInline(tokens)}</a>`;
    },
    image({ href, text }) {
      return `<span class="research-image" data-research-image="${escapeHtml(href)}" data-image-alt="${escapeHtml(text)}"><span class="research-image-status">図を読み込み中：${escapeHtml(text)}</span></span>`;
    },
  },
  extensions: [
    {
      name: "wikiSectionAnchor",
      level: "block",
      start(src) { return src.indexOf('<a id="'); },
      tokenizer(src) {
        const anchor = /^<a id="([a-zA-Z0-9][a-zA-Z0-9_.:-]{0,159})"><\/a>[ \t]*(?:\n|$)/.exec(src);
        if (anchor?.[1]) return { type: "wikiSectionAnchor", raw: anchor[0], id: anchor[1] };
        return undefined;
      },
      renderer(token) { return `<span data-research-anchor="${escapeHtml(String(token.id))}"></span>`; },
    },
    {
      name: "displayMath",
      level: "block",
      start(src) { return src.indexOf("$$"); },
      tokenizer(src) {
        const match = /^ {0,3}\$\$[ \t]*\n([\s\S]*?)\n {0,3}\$\$[ \t]*(?:\n|$)/.exec(src);
        if (match?.[1] !== undefined) return { type: "displayMath", raw: match[0], text: match[1] };
        return undefined;
      },
      renderer(token) { return mathHtml(String(token.text), true); },
    },
    {
      name: "inlineMath",
      level: "inline",
      start(src) { return src.indexOf("$"); },
      tokenizer(src) {
        const display = /^\$\$([^\n]+?)\$\$/.exec(src);
        if (display?.[1]) return { type: "inlineMath", raw: display[0], text: display[1], display: true };
        const match = /^\$(?!\s|\$)((?:\\.|[^$\\\n])+?)\$(?!\d)/.exec(src);
        if (match?.[1] && !/\s$/.test(match[1])) return { type: "inlineMath", raw: match[0], text: match[1], display: false };
        return undefined;
      },
      renderer(token) { return mathHtml(String(token.text), token.display === true); },
    },
  ],
});

/** Only inert section anchors become HTML. Links and images are resolved separately by the backend. */
export function renderResearchMarkdown(text: string): string {
  const html = markdown.parse(text, { async: false });
  return DOMPurify.sanitize(html, { FORBID_TAGS: ["script", "iframe", "object", "embed", "form", "input", "style"], FORBID_ATTR: ["src", "srcset"] });
}

interface MarkdownContext {
  workspaceRoot: string;
  documentPath?: string;
  onLink: (href: string) => Promise<void>;
  onError: (message: string) => void;
}

export function hydrateResearchMarkdown(container: HTMLElement, context: MarkdownContext): void {
  container.querySelectorAll<HTMLAnchorElement>("a[data-research-href]").forEach((link) => {
    link.addEventListener("click", (event) => {
      event.preventDefault();
      const href = link.dataset.researchHref;
      if (href) void context.onLink(href).catch((error: unknown) => {
        if (container.isConnected) context.onError(String(error));
      });
    });
  });
  // A small worker pool keeps a long review from launching hundreds of IPC calls at once.
  const figures = [...container.querySelectorAll<HTMLElement>("[data-research-image]")];
  async function loadNext(): Promise<void> {
    while (container.isConnected) {
      const figure = figures.shift();
      if (!figure) return;
      const href = figure.dataset.researchImage ?? "";
      const alt = figure.dataset.imageAlt ?? "";
      try {
        if (!context.documentPath) throw new Error("この記録には画像の参照元がありません。書き出されたノートから確認してください。");
        if (/^(?:[a-z][a-z\d+.-]*:)?\/\//i.test(href) || /^(?:data|javascript|https?|blob):/i.test(href)) {
          throw new Error("外部画像は自動で読み込みません。");
        }
        const path = await invoke<string>("resolve_research_asset", { workspaceRoot: context.workspaceRoot, documentPath: context.documentPath, href });
        if (!container.isConnected) return;
        const image = document.createElement("img");
        image.alt = alt;
        image.loading = "lazy";
        image.addEventListener("error", () => { figure.textContent = `図を表示できません：${alt}`; figure.classList.add("research-image-error"); }, { once: true });
        image.src = convertFileSrc(path);
        figure.replaceChildren(image);
      } catch (error) {
        if (!container.isConnected) return;
        figure.textContent = `図：${alt} — ${String(error)}`;
        figure.classList.add("research-image-error");
      }
    }
  }
  void Promise.all(Array.from({ length: Math.min(4, figures.length) }, loadNext));
}
