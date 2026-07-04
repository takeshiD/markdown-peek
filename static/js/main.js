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
        toc.classList.toggle("mdpeek-toc-collapsed");
        const caret = title.lastElementChild;
        if (caret) {
            caret.textContent = toc.classList.contains("mdpeek-toc-collapsed") ? "▸" : "▾";
        }
    });

    document.body.appendChild(toc);
    initScrollSpy(toc);
}

// Render the document's front matter (served in a hidden element) as a
// collapsible panel.
function initializeFrontmatter() {
    const source = document.getElementById("mdpeek-frontmatter");
    if (!source) {
        return;
    }
    const raw = (source.textContent || "").trim();
    if (!raw) {
        return;
    }

    const panel = document.createElement("section");
    panel.id = "mdpeek-frontmatter-panel";

    const title = document.createElement("div");
    title.id = "mdpeek-frontmatter-title";
    title.innerHTML = "<span>Front matter</span><span aria-hidden=\"true\">▾</span>";
    title.addEventListener("click", function () {
        panel.classList.toggle("mdpeek-collapsed");
        const caret = title.lastElementChild;
        if (caret) {
            caret.textContent = panel.classList.contains("mdpeek-collapsed") ? "▸" : "▾";
        }
    });

    const pre = document.createElement("pre");
    pre.textContent = raw;

    panel.appendChild(title);
    panel.appendChild(pre);
    document.body.appendChild(panel);
}

(function() {
    initializeTheme();
    initializeMermaid();
    initializeHighlight();
    initializeOutline();
    initializeFrontmatter();
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
            console.log("WebSocket message: " + event.data);
            if (event.data === "reload") {
                socket.close();
                window.location.reload();
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
