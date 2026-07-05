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
