function initializeMermaid() {
    window.mermaid.initialize({ startOnLoad: false, theme: "default" });
    window.mermaid.run({ querySelector: "code.language-mermaid" });
}

function initializeMathJax() {
    window.MathJax.typeset();
}

function initializeHighlight() {
    if (!window.hljs) {
        return;
    }
    // Highlight every code block except mermaid diagrams and math blocks,
    // which are handled by mermaid/MathJax respectively.
    document.querySelectorAll("pre code").forEach(function (block) {
        if (
            block.classList.contains("language-mermaid") ||
            block.classList.contains("language-math")
        ) {
            return;
        }
        window.hljs.highlightElement(block);
    });
}

const MDPEEK_THEME_KEY = "mdpeek-theme";

// Collapse state for the floating panels. Kept in module scope so it survives
// live DOM updates (#16), which rebuild these panels without a page reload.
let outlineCollapsed = false;
let frontmatterCollapsed = false;

// Lucide icons (https://lucide.dev) embedded inline so we don't depend on a CDN.
const LUCIDE_MOON =
    '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-moon"><path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/></svg>';
const LUCIDE_SUN =
    '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-sun"><circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/></svg>';
// git-branch / folder icons for the branch vs. worktree grouping (#14) and breadcrumb.
const LUCIDE_BRANCH =
    '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-git-branch"><line x1="6" x2="6" y1="3" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/></svg>';
const LUCIDE_WORKTREE =
    '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-folder"><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/></svg>';
// Icons for the diff mode/layout segmented controls (#15).
const ICON_CODE =
    '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-code"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>';
const ICON_EYE =
    '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-eye"><path d="M2.06 12.35a1 1 0 0 1 0-.7 10.75 10.75 0 0 1 19.88 0 1 1 0 0 1 0 .7 10.75 10.75 0 0 1-19.88 0Z"/><circle cx="12" cy="12" r="3"/></svg>';
const ICON_ROWS =
    '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-align-justify"><line x1="3" x2="21" y1="6" y2="6"/><line x1="3" x2="21" y1="12" y2="12"/><line x1="3" x2="21" y1="18" y2="18"/></svg>';
const ICON_COLUMNS =
    '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-columns-2"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M12 3v18"/></svg>';
const ICON_CARET =
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-chevron-down mdpeek-bc-caret"><path d="m6 9 6 6 6-6"/></svg>';

// Escape arbitrary text for safe innerHTML insertion.
function escapeText(s) {
    const d = document.createElement("div");
    d.textContent = s == null ? "" : String(s);
    return d.innerHTML;
}

// Last /api/tree payload, so the breadcrumb (#14) can be recomputed on file switch.
let treeData = null;

function currentTheme() {
    const stored = localStorage.getItem(MDPEEK_THEME_KEY);
    if (stored === "light" || stored === "dark") {
        return stored;
    }
    if (window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches) {
        return "dark";
    }
    return "light";
}

function applyTheme(theme) {
    const isDark = theme === "dark";
    const lightCss = document.getElementById("mdpeek-theme-light");
    const darkCss = document.getElementById("mdpeek-theme-dark");
    const hljsLight = document.getElementById("mdpeek-hljs-light");
    const hljsDark = document.getElementById("mdpeek-hljs-dark");
    if (lightCss) lightCss.disabled = isDark;
    if (darkCss) darkCss.disabled = !isDark;
    if (hljsLight) hljsLight.disabled = isDark;
    if (hljsDark) hljsDark.disabled = !isDark;

    const toggle = document.getElementById("mdpeek-theme-toggle");
    if (toggle) {
        // Show the icon for the theme you'd switch *to*.
        toggle.innerHTML = isDark ? LUCIDE_SUN : LUCIDE_MOON;
    }
}

function initializeTheme() {
    let theme = currentTheme();
    applyTheme(theme);
    const toggle = document.getElementById("mdpeek-theme-toggle");
    if (toggle) {
        toggle.addEventListener("click", function () {
            theme = theme === "dark" ? "light" : "dark";
            localStorage.setItem(MDPEEK_THEME_KEY, theme);
            applyTheme(theme);
        });
    }
}

// Subsequence fuzzy match: every char of `query` appears in `text` in order.
function fuzzyMatch(query, text) {
    if (!query) {
        return true;
    }
    query = query.toLowerCase();
    text = text.toLowerCase();
    let qi = 0;
    for (let i = 0; i < text.length && qi < query.length; i++) {
        if (text[i] === query[qi]) {
            qi++;
        }
    }
    return qi === query.length;
}

// Fold the flat heading list into a tree nested by heading level.
function buildOutlineTree(headings) {
    const root = { level: 0, children: [] };
    const stack = [root];
    headings.forEach(function (heading) {
        if (!heading.id) {
            return;
        }
        const level = parseInt(heading.tagName.substring(1), 10);
        const node = {
            id: heading.id,
            level: level,
            text: (heading.textContent || "").trim(),
            children: [],
        };
        // Pop until the parent is a strictly shallower heading.
        while (stack.length > 1 && stack[stack.length - 1].level >= level) {
            stack.pop();
        }
        stack[stack.length - 1].children.push(node);
        stack.push(node);
    });
    return root.children;
}

function renderOutline(nodes) {
    const ul = document.createElement("ul");
    nodes.forEach(function (node) {
        const item = document.createElement("li");
        item.className = "mdpeek-toc-item";
        const link = document.createElement("a");
        link.href = "#" + node.id;
        link.textContent = node.text;
        link.dataset.tocId = node.id;
        link.addEventListener("click", function (event) {
            event.preventDefault();
            const target = document.getElementById(node.id);
            if (target) {
                target.scrollIntoView({ behavior: "smooth", block: "start" });
                history.replaceState(null, "", "#" + node.id);
            }
        });
        item.appendChild(link);
        if (node.children.length) {
            item.appendChild(renderOutline(node.children));
        }
        ul.appendChild(item);
    });
    return ul;
}

// Show only items whose heading fuzzy-matches the query, keeping the ancestors
// of each hit visible so the path stays navigable.
function filterOutline(toc, query) {
    const items = toc.querySelectorAll("li.mdpeek-toc-item");
    if (!query) {
        items.forEach(function (li) {
            li.hidden = false;
        });
        return;
    }
    items.forEach(function (li) {
        const link = li.querySelector(":scope > a");
        const hit = link && fuzzyMatch(query, link.textContent || "");
        li.hidden = !hit;
        li.dataset.tocHit = hit ? "1" : "";
    });
    // Reveal ancestors of every hit.
    toc.querySelectorAll('li.mdpeek-toc-item[data-toc-hit="1"]').forEach(function (li) {
        let parent = li.parentElement;
        while (parent && parent !== toc) {
            if (parent.tagName === "LI") {
                parent.hidden = false;
            }
            parent = parent.parentElement;
        }
    });
}

// Highlight the outline entry for the heading currently scrolled into view.
function initScrollSpy(toc) {
    const links = new Map();
    toc.querySelectorAll("a[data-toc-id]").forEach(function (a) {
        links.set(a.dataset.tocId, a);
    });
    const headings = Array.from(links.keys())
        .map(function (id) { return document.getElementById(id); })
        .filter(Boolean);
    if (!headings.length) {
        return;
    }
    let activeId = null;
    function onScroll() {
        let current = headings[0].id;
        for (let i = 0; i < headings.length; i++) {
            if (headings[i].getBoundingClientRect().top <= 80) {
                current = headings[i].id;
            } else {
                break;
            }
        }
        if (current === activeId) {
            return;
        }
        if (activeId && links.get(activeId)) {
            links.get(activeId).classList.remove("mdpeek-toc-active");
        }
        activeId = current;
        const active = links.get(activeId);
        if (active) {
            active.classList.add("mdpeek-toc-active");
        }
    }
    document.addEventListener("scroll", onScroll, { passive: true });
    onScroll();
}

function initializeOutline() {
    // Remove any previous outline so this function can be re-run after a live
    // update (#16) rebuilds the document body.
    const existing = document.getElementById("mdpeek-toc");
    if (existing) {
        existing.remove();
    }

    const article = document.querySelector(".markdown-body");
    if (!article) {
        return;
    }
    const headings = article.querySelectorAll("h1, h2, h3, h4, h5, h6");
    // Only show the outline when there is more than one heading.
    if (headings.length < 2) {
        return;
    }

    const toc = document.createElement("nav");
    toc.id = "mdpeek-toc";

    const title = document.createElement("div");
    title.id = "mdpeek-toc-title";
    title.innerHTML = "<span>Contents</span><span aria-hidden=\"true\">▾</span>";
    toc.appendChild(title);

    const search = document.createElement("input");
    search.id = "mdpeek-toc-search";
    search.type = "search";
    search.placeholder = "Filter headings…";
    search.setAttribute("aria-label", "Filter headings");
    toc.appendChild(search);

    const tree = buildOutlineTree(headings);
    const list = renderOutline(tree);
    if (!list.children.length) {
        return;
    }
    toc.appendChild(list);

    search.addEventListener("input", function () {
        filterOutline(toc, search.value.trim());
    });
    // Don't let the collapse toggle fire when interacting with the search box.
    search.addEventListener("click", function (event) {
        event.stopPropagation();
    });

    title.addEventListener("click", function () {
        outlineCollapsed = toc.classList.toggle("mdpeek-toc-collapsed");
        const caret = title.lastElementChild;
        if (caret) {
            caret.textContent = outlineCollapsed ? "▸" : "▾";
        }
    });

    // Restore collapse state carried over from a previous render.
    if (outlineCollapsed) {
        toc.classList.add("mdpeek-toc-collapsed");
        const caret = title.lastElementChild;
        if (caret) {
            caret.textContent = "▸";
        }
    }

    document.body.appendChild(toc);
    initScrollSpy(toc);
}

// Render the document's front matter as a collapsible panel. `rawText` is the
// plain (unescaped) front matter; an empty/blank value removes the panel. Safe
// to call repeatedly — live updates (#16) rebuild the panel with fresh text.
function buildFrontmatterPanel(rawText) {
    const existing = document.getElementById("mdpeek-frontmatter-panel");
    if (existing) {
        existing.remove();
    }
    const raw = (rawText || "").trim();
    if (!raw) {
        return;
    }

    const panel = document.createElement("section");
    panel.id = "mdpeek-frontmatter-panel";

    const title = document.createElement("div");
    title.id = "mdpeek-frontmatter-title";
    title.innerHTML = "<span>Front matter</span><span aria-hidden=\"true\">▾</span>";
    title.addEventListener("click", function () {
        frontmatterCollapsed = panel.classList.toggle("mdpeek-collapsed");
        const caret = title.lastElementChild;
        if (caret) {
            caret.textContent = frontmatterCollapsed ? "▸" : "▾";
        }
    });

    const pre = document.createElement("pre");
    pre.textContent = raw;

    panel.appendChild(title);
    panel.appendChild(pre);

    // Restore collapse state carried over from a previous render.
    if (frontmatterCollapsed) {
        panel.classList.add("mdpeek-collapsed");
        title.lastElementChild.textContent = "▸";
    }

    document.body.appendChild(panel);
}

// Initial front matter comes from a hidden element the server injects.
function initializeFrontmatter() {
    const source = document.getElementById("mdpeek-frontmatter");
    buildFrontmatterPanel(source ? source.textContent : "");
}

// ---------------------------------------------------------------------------
// Live update (#16): patch changed blocks in place instead of full-reloading.
// ---------------------------------------------------------------------------

const MDPEEK_AUTOSCROLL_KEY = "mdpeek-autoscroll";

// Clean (pre-highlight) HTML of the last rendered body. We diff server HTML
// against this snapshot rather than the live DOM, whose blocks get mutated by
// highlight.js / mermaid / MathJax after rendering.
let lastBodyHTML = "";

function autoScrollEnabled() {
    return localStorage.getItem(MDPEEK_AUTOSCROLL_KEY) === "1";
}

function parseFragment(html) {
    const tmp = document.createElement("div");
    tmp.innerHTML = html;
    return tmp;
}

// Re-run syntax highlighting on freshly inserted blocks only.
function highlightWithin(nodes) {
    if (!window.hljs) {
        return;
    }
    nodes.forEach(function (node) {
        if (!node.querySelectorAll) {
            return;
        }
        node.querySelectorAll("pre code").forEach(function (block) {
            if (
                block.classList.contains("language-mermaid") ||
                block.classList.contains("language-math")
            ) {
                return;
            }
            window.hljs.highlightElement(block);
        });
    });
}

function mermaidWithin(nodes) {
    if (!window.mermaid) {
        return;
    }
    const els = [];
    nodes.forEach(function (node) {
        if (!node.querySelectorAll) {
            return;
        }
        node.querySelectorAll("code.language-mermaid").forEach(function (el) {
            els.push(el);
        });
    });
    if (els.length) {
        try {
            window.mermaid.run({ nodes: els });
        } catch (e) {
            console.log("mermaid update error", e);
        }
    }
}

function typesetWithin(nodes) {
    if (window.MathJax && typeof window.MathJax.typesetPromise === "function") {
        window.MathJax.typesetPromise(nodes).catch(function (e) {
            console.log("MathJax update error", e);
        });
    }
}

// Briefly highlight changed blocks with a fading background.
function flashChanged(nodes) {
    nodes.forEach(function (node) {
        if (!node.classList) {
            return;
        }
        node.classList.remove("mdpeek-changed");
        // Force reflow so re-adding the class restarts the animation.
        void node.offsetWidth;
        node.classList.add("mdpeek-changed");
        setTimeout(function () {
            node.classList.remove("mdpeek-changed");
        }, 1600);
    });
}

// Diff the new body against the last clean snapshot and patch only the changed
// top-level blocks into the live article. Returns the newly inserted nodes.
function patchArticle(article, newHTML) {
    const oldClean = Array.from(parseFragment(lastBodyHTML).children).map(function (e) {
        return e.outerHTML;
    });
    const newNodes = Array.from(parseFragment(newHTML).children);
    const newClean = newNodes.map(function (e) {
        return e.outerHTML;
    });

    // If the live block count drifted from our snapshot (unexpected external
    // mutation), fall back to a full replace to stay correct.
    if (article.children.length !== oldClean.length) {
        article.innerHTML = newHTML;
        return Array.from(article.children);
    }

    const n = oldClean.length;
    const m = newClean.length;
    let a = 0;
    while (a < n && a < m && oldClean[a] === newClean[a]) {
        a++;
    }
    let bOld = n - 1;
    let bNew = m - 1;
    while (bOld >= a && bNew >= a && oldClean[bOld] === newClean[bNew]) {
        bOld--;
        bNew--;
    }

    const live = Array.from(article.children);
    const ref = live[bOld + 1] || null;
    for (let i = a; i <= bOld; i++) {
        article.removeChild(live[i]);
    }
    const changed = [];
    for (let i = a; i <= bNew; i++) {
        article.insertBefore(newNodes[i], ref);
        changed.push(newNodes[i]);
    }
    return changed;
}

function applyUpdate(newHTML, frontmatter) {
    const article = document.querySelector(".markdown-body");
    if (!article) {
        return;
    }
    const changed = patchArticle(article, newHTML);
    lastBodyHTML = newHTML;

    highlightWithin(changed);
    mermaidWithin(changed);
    typesetWithin(changed);
    initializeOutline();
    refreshTocState();
    buildFrontmatterPanel(frontmatter);
    flashChanged(changed);

    if (changed.length && autoScrollEnabled()) {
        changed[0].scrollIntoView({ behavior: "smooth", block: "center" });
    }
}

// ---------------------------------------------------------------------------
// TOC visibility toggle (#13): let the user show/hide the outline regardless of
// viewport width, persisted across reloads and live updates.
// ---------------------------------------------------------------------------

const MDPEEK_TOC_KEY = "mdpeek-toc-visible";
// Viewport width above which the outline shows by default (matches the CSS
// `@media (max-width: 1280px)` rule that hides it on narrow screens).
const MDPEEK_TOC_WIDTH = 1280;

// Whether the outline should be visible: explicit user choice wins, otherwise
// fall back to the width-based default.
function tocShouldShow() {
    const pref = localStorage.getItem(MDPEEK_TOC_KEY);
    if (pref === "visible") {
        return true;
    }
    if (pref === "hidden") {
        return false;
    }
    return window.innerWidth > MDPEEK_TOC_WIDTH;
}

// Reflect stored preference onto body classes (which override the CSS default)
// and the toggle button's pressed/visible state. Safe to call repeatedly.
function refreshTocState() {
    const pref = localStorage.getItem(MDPEEK_TOC_KEY);
    document.body.classList.toggle("mdpeek-toc-user-visible", pref === "visible");
    document.body.classList.toggle("mdpeek-toc-user-hidden", pref === "hidden");

    const btn = document.getElementById("mdpeek-toc-toggle");
    if (!btn) {
        return;
    }
    // The button only makes sense when an outline exists (headings >= 2).
    const toc = document.getElementById("mdpeek-toc");
    if (!toc) {
        btn.hidden = true;
        return;
    }
    btn.hidden = false;
    const shown = tocShouldShow();
    btn.classList.toggle("mdpeek-active", shown);
    btn.setAttribute("aria-pressed", shown ? "true" : "false");
}

function initializeTocToggle() {
    const btn = document.getElementById("mdpeek-toc-toggle");
    if (btn) {
        btn.addEventListener("click", function () {
            localStorage.setItem(MDPEEK_TOC_KEY, tocShouldShow() ? "hidden" : "visible");
            refreshTocState();
        });
        // When no explicit choice is stored, the default follows width.
        window.addEventListener("resize", refreshTocState, { passive: true });
    }
    refreshTocState();
}

// Toolbar toggle for "scroll to first change on update".
function initializeAutoScrollToggle() {
    const btn = document.getElementById("mdpeek-autoscroll-toggle");
    if (!btn) {
        return;
    }
    function sync() {
        const on = autoScrollEnabled();
        btn.classList.toggle("mdpeek-active", on);
        btn.setAttribute("aria-pressed", on ? "true" : "false");
    }
    sync();
    btn.addEventListener("click", function () {
        localStorage.setItem(MDPEEK_AUTOSCROLL_KEY, autoScrollEnabled() ? "0" : "1");
        sync();
    });
}

// ---------------------------------------------------------------------------
// Repository + worktree explorer sidebar (#14).
// ---------------------------------------------------------------------------

const MDPEEK_SIDEBAR_KEY = "mdpeek-sidebar-open";
const MDPEEK_GROUPBY_KEY = "mdpeek-tree-groupby"; // "worktree" | "branch"

// Active single file (for restoring after closing a diff), and the first file
// picked for a two-file comparison (#15).
let lastActivePath = null;
let pendingCompare = null;

function sidebarGroupBy() {
    return localStorage.getItem(MDPEEK_GROUPBY_KEY) === "branch" ? "branch" : "worktree";
}

function applySidebarVisibility() {
    const open = localStorage.getItem(MDPEEK_SIDEBAR_KEY) === "1";
    document.body.classList.toggle("mdpeek-sidebar-open", open);
    const btn = document.getElementById("mdpeek-sidebar-toggle");
    if (btn) {
        btn.classList.toggle("mdpeek-active", open);
        btn.setAttribute("aria-pressed", open ? "true" : "false");
    }
}

// Mark the sidebar entry for `path` as active (used by both sidebar clicks and
// breadcrumb-driven selection).
function markActiveInSidebar(path) {
    document.querySelectorAll("#mdpeek-sidebar a.mdpeek-file-active").forEach(function (a) {
        a.classList.remove("mdpeek-file-active");
    });
    if (!path) {
        return;
    }
    document.querySelectorAll("#mdpeek-sidebar a[data-mdpeek-path]").forEach(function (a) {
        if (a.getAttribute("data-mdpeek-path") === path) {
            a.classList.add("mdpeek-file-active");
        }
    });
}

function selectFile(path) {
    fetch("/api/select", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ path: path }),
    }).then(function (r) {
        if (!r.ok) {
            console.log("select rejected", r.status);
            return;
        }
        lastActivePath = path;
        // The server pushes a live update over the WebSocket; just move the
        // active marker here (by path, so breadcrumb-driven selects also mark it).
        markActiveInSidebar(path);
        updateBreadcrumb();
    }).catch(function (e) {
        console.log("select error", e);
    });
}

function buildSidebar(data) {
    const tree = data && data.tree;
    const btn = document.getElementById("mdpeek-sidebar-toggle");
    const hasFiles = tree && tree.groups && tree.groups.some(function (g) {
        return g.files && g.files.length;
    });
    if (!hasFiles) {
        if (btn) {
            btn.hidden = true;
        }
        return;
    }
    if (btn) {
        btn.hidden = false;
    }

    const existing = document.getElementById("mdpeek-sidebar");
    if (existing) {
        existing.remove();
    }

    treeData = data;
    if (data.active) {
        lastActivePath = data.active;
    }
    const groupBy = sidebarGroupBy();
    const aside = document.createElement("aside");
    aside.id = "mdpeek-sidebar";

    const header = document.createElement("div");
    header.id = "mdpeek-sidebar-header";

    // Group-by switch (worktree directory name vs. checked-out branch) sits at
    // the header's left; the sidebar toggle floats at the top-right.
    const groupToggle = document.createElement("button");
    groupToggle.type = "button";
    groupToggle.id = "mdpeek-groupby-toggle";
    groupToggle.innerHTML =
        (groupBy === "branch" ? LUCIDE_BRANCH : LUCIDE_WORKTREE) +
        "<span>" + (groupBy === "branch" ? "branch" : "worktree") + "</span>";
    groupToggle.title = "Switch grouping (worktree / branch)";
    groupToggle.addEventListener("click", function () {
        localStorage.setItem(MDPEEK_GROUPBY_KEY, groupBy === "branch" ? "worktree" : "branch");
        buildSidebar(data);
    });
    header.appendChild(groupToggle);
    aside.appendChild(header);

    // The file list scrolls independently of the header row.
    const content = document.createElement("div");
    content.id = "mdpeek-sidebar-content";

    tree.groups.forEach(function (group) {
        if (!group.files || !group.files.length) {
            return;
        }
        const section = document.createElement("div");
        section.className = "mdpeek-sidebar-group";
        const label = document.createElement("div");
        label.className = "mdpeek-sidebar-group-title";
        const branchLabel = group.branch || group.name;
        label.textContent = groupBy === "branch" ? branchLabel : group.name;
        section.appendChild(label);

        const ul = document.createElement("ul");
        group.files.forEach(function (file) {
            const li = document.createElement("li");
            li.className = "mdpeek-file-row";
            const a = document.createElement("a");
            a.href = "#";
            a.textContent = file.rel;
            a.title = file.path;
            a.setAttribute("data-mdpeek-path", file.path);
            if (data.active && file.path === data.active) {
                a.classList.add("mdpeek-file-active");
            }
            a.addEventListener("click", function (event) {
                event.preventDefault();
                selectFile(file.path);
            });
            // Compare affordance (#15): pick two files (e.g. the same path in
            // two worktrees) to diff.
            const cmp = document.createElement("button");
            cmp.type = "button";
            cmp.className = "mdpeek-compare";
            cmp.title = "Select for compare";
            cmp.textContent = "⇄";
            cmp.addEventListener("click", function (event) {
                event.preventDefault();
                event.stopPropagation();
                toggleCompare(file, cmp);
            });
            li.appendChild(a);
            li.appendChild(cmp);
            ul.appendChild(li);
        });
        section.appendChild(ul);
        content.appendChild(section);
    });

    aside.appendChild(content);
    document.body.appendChild(aside);
    updateBreadcrumb();
}

// ---------------------------------------------------------------------------
// Breadcrumb (#14): show which worktree/branch the open file belongs to, e.g.
// "layer1-viewer > README.md" (branch or worktree name per the grouping toggle).
// ---------------------------------------------------------------------------

// The group whose root is the longest prefix of `path` (worktrees can nest, so
// the most specific root wins).
function groupForPath(path) {
    if (!treeData || !treeData.tree || !path) {
        return null;
    }
    let best = null;
    treeData.tree.groups.forEach(function (g) {
        if (path === g.root || path.indexOf(g.root + "/") === 0) {
            if (!best || g.root.length > best.root.length) {
                best = g;
            }
        }
    });
    return best;
}

function activeGroup() {
    return groupForPath(lastActivePath);
}

// A file's worktree/branch scope and its path relative to that worktree.
function fileLabel(path) {
    const g = groupForPath(path);
    let rel = path ? path.split("/").pop() : "";
    let scope = "";
    if (g) {
        const f = g.files.filter(function (x) { return x.path === path; })[0];
        if (f) {
            rel = f.rel;
        }
        scope = g.branch ? g.name + " · " + g.branch : g.name;
    }
    return { scope: scope, rel: rel };
}

function activeFileLabel(group) {
    if (group) {
        const f = group.files.filter(function (x) { return x.path === lastActivePath; })[0];
        if (f) {
            return f.rel;
        }
    }
    return lastActivePath ? lastActivePath.split("/").pop() : "";
}

// --- breadcrumb dropdown menu ---
let bcOpenMenu = null;

function closeBcMenu() {
    if (bcOpenMenu) {
        bcOpenMenu.remove();
        bcOpenMenu = null;
        document.removeEventListener("click", onBcOutsideClick, true);
    }
}

function onBcOutsideClick(e) {
    if (bcOpenMenu && !bcOpenMenu.contains(e.target)) {
        closeBcMenu();
    }
}

// Open a dropdown anchored under `anchorEl`. `items` = [{html, active, value}];
// `onPick(value)` fires on selection.
function openBcMenu(anchorEl, items, onPick) {
    const wasOpen = bcOpenMenu;
    closeBcMenu();
    if (wasOpen && wasOpen.dataset.anchor === anchorEl.id) {
        return; // clicking the same trigger again just closes it
    }
    const menu = document.createElement("div");
    menu.className = "mdpeek-bc-menu";
    menu.dataset.anchor = anchorEl.id;
    items.forEach(function (it) {
        const b = document.createElement("button");
        b.type = "button";
        b.className = "mdpeek-bc-menu-item" + (it.active ? " is-active" : "");
        b.innerHTML = it.html;
        b.addEventListener("click", function (ev) {
            ev.stopPropagation();
            closeBcMenu();
            onPick(it.value);
        });
        menu.appendChild(b);
    });
    const r = anchorEl.getBoundingClientRect();
    menu.style.left = r.left + "px";
    menu.style.top = r.bottom + 4 + "px";
    document.body.appendChild(menu);
    bcOpenMenu = menu;
    // Defer so this same click doesn't immediately close the menu.
    setTimeout(function () {
        document.addEventListener("click", onBcOutsideClick, true);
    }, 0);
}

// Switch to another worktree/branch, keeping the same relative file when it
// exists there (otherwise the group's first file).
function switchToGroup(g) {
    if (!g || !g.files.length) {
        return;
    }
    const curRel = activeFileLabel(activeGroup());
    const same = g.files.filter(function (f) { return f.rel === curRel; })[0];
    const target = same || g.files[0];
    selectFile(target.path);
}

function openGroupMenu(anchor) {
    if (!treeData || !treeData.tree) {
        return;
    }
    const groupBy = sidebarGroupBy();
    const cur = activeGroup();
    const items = treeData.tree.groups
        .filter(function (g) { return g.files.length; })
        .map(function (g) {
            const label = groupBy === "branch" ? (g.branch || g.name) : g.name;
            const icon = groupBy === "branch" ? LUCIDE_BRANCH : LUCIDE_WORKTREE;
            return {
                value: g,
                active: cur && g.root === cur.root,
                html: icon + "<span>" + escapeText(label) + "</span>",
            };
        });
    openBcMenu(anchor, items, switchToGroup);
}

function openFileMenu(anchor) {
    const g = activeGroup();
    if (!g) {
        return;
    }
    const items = g.files.map(function (f) {
        return {
            value: f,
            active: f.path === lastActivePath,
            html: "<span>" + escapeText(f.rel) + "</span>",
        };
    });
    openBcMenu(anchor, items, function (f) { selectFile(f.path); });
}

function updateBreadcrumb() {
    const bc = document.getElementById("mdpeek-breadcrumb");
    if (!bc) {
        return;
    }
    closeBcMenu();
    const group = activeGroup();
    const file = activeFileLabel(group);
    if (!file) {
        bc.hidden = true;
        bc.innerHTML = "";
        return;
    }
    bc.hidden = false;
    bc.innerHTML = "";

    if (group) {
        const groupBy = sidebarGroupBy();
        const label = groupBy === "branch" ? (group.branch || group.name) : group.name;
        const icon = groupBy === "branch" ? LUCIDE_BRANCH : LUCIDE_WORKTREE;
        const gbtn = document.createElement("button");
        gbtn.type = "button";
        gbtn.id = "mdpeek-bc-group-btn";
        gbtn.className = "mdpeek-bc-btn mdpeek-bc-group";
        gbtn.title = "Switch worktree/branch";
        gbtn.innerHTML = icon + "<span>" + escapeText(label) + "</span>" + ICON_CARET;
        gbtn.addEventListener("click", function (e) {
            e.stopPropagation();
            openGroupMenu(gbtn);
        });
        bc.appendChild(gbtn);
        const sep = document.createElement("span");
        sep.className = "mdpeek-bc-sep";
        sep.setAttribute("aria-hidden", "true");
        sep.textContent = "›";
        bc.appendChild(sep);
    }

    const fbtn = document.createElement("button");
    fbtn.type = "button";
    fbtn.id = "mdpeek-bc-file-btn";
    fbtn.className = "mdpeek-bc-btn mdpeek-bc-file";
    fbtn.title = "Switch file";
    fbtn.innerHTML = "<span>" + escapeText(file) + "</span>" + ICON_CARET;
    fbtn.addEventListener("click", function (e) {
        e.stopPropagation();
        openFileMenu(fbtn);
    });
    bc.appendChild(fbtn);
}

function initializeBreadcrumb() {
    if (document.getElementById("mdpeek-breadcrumb")) {
        return;
    }
    const bc = document.createElement("nav");
    bc.id = "mdpeek-breadcrumb";
    bc.hidden = true;
    document.body.appendChild(bc);
}

function initializeSidebar() {
    const btn = document.getElementById("mdpeek-sidebar-toggle");
    if (btn) {
        btn.addEventListener("click", function () {
            const open = localStorage.getItem(MDPEEK_SIDEBAR_KEY) === "1";
            localStorage.setItem(MDPEEK_SIDEBAR_KEY, open ? "0" : "1");
            applySidebarVisibility();
        });
    }
    applySidebarVisibility();
    fetch("/api/tree")
        .then(function (r) { return r.json(); })
        .then(function (data) { buildSidebar(data); })
        .catch(function (e) { console.log("tree fetch failed", e); });
}

// ---------------------------------------------------------------------------
// Two-file diff view (#15).
// ---------------------------------------------------------------------------

function clearPendingCompare() {
    if (pendingCompare && pendingCompare.el) {
        pendingCompare.el.classList.remove("mdpeek-compare-pending");
    }
    pendingCompare = null;
}

// First compare click marks a file; the second (on a different file) opens the
// diff. Clicking the same file again cancels.
function toggleCompare(file, el) {
    if (pendingCompare && pendingCompare.path === file.path) {
        clearPendingCompare();
        return;
    }
    if (!pendingCompare) {
        pendingCompare = { path: file.path, el: el };
        el.classList.add("mdpeek-compare-pending");
        return;
    }
    const a = pendingCompare.path;
    const b = file.path;
    clearPendingCompare();
    openDiff(a, b);
}

// Current diff view state: the two paths plus mode (source/rendered) and layout
// (unified/split), so toggles re-request and live re-diffs stay consistent.
let diffState = null;
// Previous diff body HTML, to flash the rows/blocks that changed on live re-diff.
let lastDiffHtml = null;

function openDiff(a, b) {
    diffState = { a: a, b: b, mode: "source", layout: "unified" };
    lastDiffHtml = null;
    ensureDiffView();
    requestDiff();
}

function requestDiff() {
    if (!diffState) {
        return;
    }
    renderDiffFiles();
    renderDiffControls();
    fetch("/api/diff", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(diffState),
    })
        .then(function (r) { return r.ok ? r.json() : Promise.reject(r.status); })
        // Manual (re)request: replace without a change flash.
        .then(function (d) { applyDiffHtml(d.html, false); })
        .catch(function (e) { console.log("diff error", e); });
}

// The diffable units (table rows, or rendered blocks) inside a container.
function diffUnits(container) {
    return Array.from(
        container.querySelectorAll(
            "table.mdpeek-diff > tbody > tr, .mdpeek-rdiff > .mdpeek-rdiff-block"
        )
    );
}

// Indices (in the new content) of the units that changed vs the old content,
// via a front/back trim so unchanged rows aren't flagged.
function diffUnitChanges(oldHtml, newHtml) {
    const o = diffUnits(parseFragment(oldHtml)).map(function (e) { return e.outerHTML; });
    const n = diffUnits(parseFragment(newHtml)).map(function (e) { return e.outerHTML; });
    let a = 0;
    while (a < o.length && a < n.length && o[a] === n[a]) {
        a++;
    }
    let bo = o.length - 1;
    let bn = n.length - 1;
    while (bo >= a && bn >= a && o[bo] === n[bn]) {
        bo--;
        bn--;
    }
    const idx = [];
    for (let i = a; i <= bn; i++) {
        idx.push(i);
    }
    return idx;
}

// Set the diff body; re-highlight code in rendered mode. When `flash` is set
// (a live re-diff), briefly highlight the rows/blocks that changed (#16).
function applyDiffHtml(html, flash) {
    const body = document.getElementById("mdpeek-diff-body");
    if (!body) {
        return;
    }
    let changedIdx = [];
    if (flash && lastDiffHtml) {
        changedIdx = diffUnitChanges(lastDiffHtml, html);
    }
    body.innerHTML = html;
    lastDiffHtml = html;
    if (diffState && diffState.mode === "rendered") {
        highlightWithin([body]);
    }
    if (changedIdx.length) {
        const units = diffUnits(body);
        flashChanged(changedIdx.map(function (i) { return units[i]; }).filter(Boolean));
    }
}

// (Re)build the A ⇄ B file labels in the diff bar, coloured to match the diff
// (left = A / deletions, right = B / additions), with a swap button.
function renderDiffFiles() {
    const el = document.getElementById("mdpeek-diff-files");
    if (!el || !diffState) {
        return;
    }
    el.innerHTML = "";
    el.appendChild(fileLabelEl(fileLabel(diffState.a), "mdpeek-diff-a"));

    const swap = document.createElement("button");
    swap.type = "button";
    swap.id = "mdpeek-diff-swap";
    swap.title = "Swap sides";
    swap.setAttribute("aria-label", "Swap left/right");
    swap.textContent = "⇄";
    swap.addEventListener("click", function () {
        const t = diffState.a;
        diffState.a = diffState.b;
        diffState.b = t;
        lastDiffHtml = null; // sides changed; don't flash the swap as edits
        requestDiff();
    });
    el.appendChild(swap);

    el.appendChild(fileLabelEl(fileLabel(diffState.b), "mdpeek-diff-b"));
}

function fileLabelEl(label, cls) {
    const span = document.createElement("span");
    span.className = "mdpeek-diff-file " + cls;
    let html = "";
    if (label.scope) {
        html +=
            '<span class="mdpeek-diff-scope">' + LUCIDE_WORKTREE +
            "<span>" + escapeText(label.scope) + "</span></span>";
    }
    html += '<span class="mdpeek-diff-rel">' + escapeText(label.rel) + "</span>";
    span.innerHTML = html;
    span.title = (label.scope ? label.scope + " · " : "") + label.rel;
    return span;
}

// Build one shadcn-style segmented control (a pill with the active option
// highlighted). `options` is [{value, label, icon}]; `onSelect(value)` fires on
// click.
function makeSegment(current, options, onSelect) {
    const seg = document.createElement("div");
    seg.className = "mdpeek-seg";
    seg.setAttribute("role", "group");
    options.forEach(function (opt) {
        const item = document.createElement("button");
        item.type = "button";
        item.className = "mdpeek-seg-item" + (opt.value === current ? " is-active" : "");
        item.setAttribute("aria-pressed", opt.value === current ? "true" : "false");
        item.innerHTML = opt.icon + "<span>" + opt.label + "</span>";
        item.addEventListener("click", function () {
            if (opt.value !== current) {
                onSelect(opt.value);
            }
        });
        seg.appendChild(item);
    });
    return seg;
}

// (Re)build the diff-bar segmented controls to reflect the current diffState.
function renderDiffControls() {
    const controls = document.getElementById("mdpeek-diff-controls");
    if (!controls || !diffState) {
        return;
    }
    controls.innerHTML = "";
    controls.appendChild(
        makeSegment(diffState.mode, [
            { value: "source", label: "Source", icon: ICON_CODE },
            { value: "rendered", label: "Rendered", icon: ICON_EYE },
        ], function (v) { diffState.mode = v; requestDiff(); })
    );
    controls.appendChild(
        makeSegment(diffState.layout, [
            { value: "unified", label: "Unified", icon: ICON_ROWS },
            { value: "split", label: "Split", icon: ICON_COLUMNS },
        ], function (v) { diffState.layout = v; requestDiff(); })
    );
}

function ensureDiffView() {
    let view = document.getElementById("mdpeek-diff-view");
    if (!view) {
        view = document.createElement("div");
        view.id = "mdpeek-diff-view";

        const bar = document.createElement("div");
        bar.id = "mdpeek-diff-bar";

        // A ⇄ B file labels (with worktree/branch) rebuilt by renderDiffFiles().
        const files = document.createElement("div");
        files.id = "mdpeek-diff-files";
        files.className = "mdpeek-diff-bar-files";

        // Segmented controls (source/rendered, unified/split) are rebuilt from
        // diffState by renderDiffControls().
        const controls = document.createElement("div");
        controls.id = "mdpeek-diff-controls";
        controls.className = "mdpeek-diff-bar-controls";

        const close = document.createElement("button");
        close.type = "button";
        close.id = "mdpeek-diff-close";
        close.textContent = "✕";
        close.title = "Close diff";
        close.addEventListener("click", closeDiff);

        bar.appendChild(files);
        bar.appendChild(controls);
        bar.appendChild(close);
        const body = document.createElement("div");
        body.id = "mdpeek-diff-body";
        view.appendChild(bar);
        view.appendChild(body);
        document.body.appendChild(view);
    }
    document.body.classList.add("mdpeek-diff-open");
}

function closeDiff() {
    document.body.classList.remove("mdpeek-diff-open");
    diffState = null;
    lastDiffHtml = null;
    const view = document.getElementById("mdpeek-diff-view");
    if (view) {
        view.remove();
    }
    // Resume single-file watching/preview on the last active file.
    if (lastActivePath) {
        selectFile(lastActivePath);
    }
}

(function() {
    const article = document.querySelector(".markdown-body");
    // Snapshot the clean server HTML before highlight/mermaid mutate the DOM.
    lastBodyHTML = article ? article.innerHTML : "";

    initializeTheme();
    initializeMermaid();
    initializeHighlight();
    initializeOutline();
    initializeFrontmatter();
    initializeTocToggle();
    initializeAutoScrollToggle();
    initializeBreadcrumb();
    initializeSidebar();
    // initializeMathJax();

    var RECONNECT_INTERVAL_MS = 3000;
    var reconnectTimer = null;

    function connectWebSocket() {
        var wsUrl = "ws://" + window.location.host + "/ws";
        console.log("Creating WebSocket: " + wsUrl);
        var socket = new WebSocket(wsUrl);

        socket.onopen = function(event) {
            console.log("WebSocket open: " + event.type);
            if (reconnectTimer !== null) {
                clearTimeout(reconnectTimer);
                reconnectTimer = null;
            }
        };

        socket.onmessage = function(event) {
            var msg;
            try {
                msg = JSON.parse(event.data);
            } catch (e) {
                // Backwards-compatible full reload for the old "reload" signal.
                if (event.data === "reload") {
                    socket.close();
                    window.location.reload();
                }
                return;
            }
            if (msg && msg.type === "update") {
                applyUpdate(msg.html, msg.frontmatter);
            } else if (msg && msg.type === "diff-update") {
                // Live re-diff (#15): refresh the diff view if it is open, and
                // flash the rows/blocks that changed (#16).
                if (document.body.classList.contains("mdpeek-diff-open")) {
                    applyDiffHtml(msg.html, true);
                }
            }
        };

        socket.onerror = function(event) {
            console.log("WebSocket error: " + event.type);
        };

        socket.onclose = function(event) {
            console.log("WebSocket closed (code=" + event.code + "). Reconnecting in " + RECONNECT_INTERVAL_MS + "ms...");
            reconnectTimer = setTimeout(connectWebSocket, RECONNECT_INTERVAL_MS);
        };
    }

    connectWebSocket();
}());
